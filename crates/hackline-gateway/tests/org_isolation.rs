//! Goal 7 — cross-org isolation. Two orgs, one device each; every
//! `_in_org` repository entry point that handlers call refuses to
//! return / mutate / count rows that belong to the other org.
//!
//! The handlers translate `GatewayError::NotFound` into `404
//! not_found` (see `error.rs`). Returning 404 — never 403 — for
//! cross-org reads is the SCOPE.md §13 Phase 4 design rule: a
//! third-party operator must not be able to enumerate ids by
//! distinguishing "row missing" from "row exists in another org"
//! via status code.

use hackline_gateway::db::{audit, devices, migrations, orgs, pool, tunnels, users};
use hackline_gateway::error::GatewayError;

fn tempdir() -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "hackline-org-iso-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn cross_org_isolation() -> anyhow::Result<()> {
    let tmp = tempdir();
    let db = pool::open(&tmp.join("gateway.db"))?;
    let conn = db.get()?;
    migrations::run(&conn)?;

    // V005 seeds org id=1 (slug "default"). Add a second org.
    let acme = orgs::insert(&conn, "acme", "Acme")?;
    assert_ne!(acme.id, orgs::DEFAULT_ORG_ID);

    // One device per org.
    let dev_default = devices::insert(&conn, orgs::DEFAULT_ORG_ID, "aa11", "default-dev")?;
    let dev_acme = devices::insert(&conn, acme.id, "bb22", "acme-dev")?;

    // Same-org reads succeed; cross-org reads return NotFound.
    assert_eq!(
        devices::get_in_org(&conn, orgs::DEFAULT_ORG_ID, dev_default.id)?.zid,
        "aa11"
    );
    assert_eq!(
        devices::get_in_org(&conn, acme.id, dev_acme.id)?.zid,
        "bb22"
    );
    assert!(matches!(
        devices::get_in_org(&conn, orgs::DEFAULT_ORG_ID, dev_acme.id),
        Err(GatewayError::NotFound)
    ));
    assert!(matches!(
        devices::get_in_org(&conn, acme.id, dev_default.id),
        Err(GatewayError::NotFound)
    ));

    // List is per-org, never leaks across.
    let default_devs = devices::list_in_org(&conn, orgs::DEFAULT_ORG_ID)?;
    let acme_devs = devices::list_in_org(&conn, acme.id)?;
    assert_eq!(default_devs.len(), 1);
    assert_eq!(default_devs[0].id, dev_default.id);
    assert_eq!(acme_devs.len(), 1);
    assert_eq!(acme_devs[0].id, dev_acme.id);

    // Cross-org delete is a no-op (returns false), never affects the other org.
    assert!(!devices::delete_in_org(
        &conn,
        orgs::DEFAULT_ORG_ID,
        dev_acme.id
    )?);
    assert!(devices::get_in_org(&conn, acme.id, dev_acme.id).is_ok());

    // Same-org delete works.
    assert!(devices::delete_in_org(&conn, acme.id, dev_acme.id)?);
    assert!(matches!(
        devices::get_in_org(&conn, acme.id, dev_acme.id),
        Err(GatewayError::NotFound)
    ));

    Ok(())
}

#[test]
fn cross_org_users_isolated() -> anyhow::Result<()> {
    let tmp = tempdir();
    let db = pool::open(&tmp.join("gateway.db"))?;
    let conn = db.get()?;
    migrations::run(&conn)?;

    let acme = orgs::insert(&conn, "acme", "Acme")?;

    let u_default = users::insert(&conn, orgs::DEFAULT_ORG_ID, "alice", "owner", "hash-alice")?;
    let u_acme = users::insert(&conn, acme.id, "bob", "owner", "hash-bob")?;

    let default_users = users::list_in_org(&conn, orgs::DEFAULT_ORG_ID)?;
    let acme_users = users::list_in_org(&conn, acme.id)?;
    assert_eq!(default_users.len(), 1);
    assert_eq!(default_users[0].id, u_default.id);
    assert_eq!(acme_users.len(), 1);
    assert_eq!(acme_users[0].id, u_acme.id);

    // Cross-org delete refused.
    assert!(!users::delete_in_org(
        &conn,
        orgs::DEFAULT_ORG_ID,
        u_acme.id
    )?);
    assert!(users::list_in_org(&conn, acme.id)?.len() == 1);

    Ok(())
}

#[test]
fn cross_org_tunnels_and_audit_isolated() -> anyhow::Result<()> {
    let mut tmp = tempdir();
    tmp.push("gateway.db");
    let db = pool::open(&tmp)?;
    let conn = db.get()?;
    migrations::run(&conn)?;

    let acme = orgs::insert(&conn, "acme", "Acme")?;
    let dev_default = devices::insert(&conn, orgs::DEFAULT_ORG_ID, "aa11", "default-dev")?;
    let dev_acme = devices::insert(&conn, acme.id, "bb22", "acme-dev")?;

    // One tunnel per device.
    let t_default = tunnels::insert(&conn, dev_default.id, "tcp", 22, None, Some(2222))?;
    let t_acme = tunnels::insert(&conn, dev_acme.id, "tcp", 22, None, Some(2223))?;

    let default_tunnels = tunnels::list_in_org(&conn, orgs::DEFAULT_ORG_ID)?;
    let acme_tunnels = tunnels::list_in_org(&conn, acme.id)?;
    assert_eq!(default_tunnels.len(), 1);
    assert_eq!(default_tunnels[0].id, t_default.id);
    assert_eq!(acme_tunnels.len(), 1);
    assert_eq!(acme_tunnels[0].id, t_acme.id);

    assert!(matches!(
        tunnels::delete_in_org(&conn, orgs::DEFAULT_ORG_ID, t_acme.id),
        Ok(false)
    ));

    // Audit rows scoped per org.
    audit::insert(
        &conn,
        None,
        Some(dev_default.id),
        None,
        "device.create",
        None,
    )?;
    audit::insert(&conn, None, Some(dev_acme.id), None, "device.create", None)?;

    let default_audit = audit::list_recent(&conn, orgs::DEFAULT_ORG_ID, 50)?;
    let acme_audit = audit::list_recent(&conn, acme.id, 50)?;
    assert_eq!(default_audit.len(), 1);
    assert_eq!(default_audit[0].device_id, Some(dev_default.id));
    assert_eq!(acme_audit.len(), 1);
    assert_eq!(acme_audit[0].device_id, Some(dev_acme.id));

    Ok(())
}
