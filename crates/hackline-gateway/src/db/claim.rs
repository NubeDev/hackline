//! `claim_pending` row + the atomic claim consumption transaction.

use rusqlite::{params, Connection};

use crate::auth::token::{self, TokenPair};
use crate::db::orgs;
use crate::error::GatewayError;

/// Insert a pending claim row if the `users` table is empty AND no
/// pending claim already exists. Returns `Some(raw_token)` if a new
/// claim was created, `None` if one already exists or users exist.
pub fn ensure_pending(conn: &Connection) -> Result<Option<String>, GatewayError> {
    let user_count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
    if user_count > 0 {
        return Ok(None);
    }

    let pending_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM claim_pending", [], |r| r.get(0))?;
    if pending_count > 0 {
        return Ok(None);
    }

    let pair = token::generate();
    conn.execute(
        "INSERT INTO claim_pending (id, token_hash, created_at) VALUES (1, ?1, unixepoch())",
        params![pair.hash],
    )?;
    Ok(Some(pair.raw))
}

/// Check whether a pending claim exists.
pub fn is_pending(conn: &Connection) -> Result<bool, GatewayError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM claim_pending", [], |r| r.get(0))?;
    Ok(count > 0)
}

/// Check whether any users exist (i.e. the gateway has been claimed).
pub fn is_claimed(conn: &Connection) -> Result<bool, GatewayError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
    Ok(count > 0)
}

/// Atomically consume the pending claim: verify the token, delete
/// the pending row, insert the owner user with a new bearer token.
/// Returns the bearer token pair on success.
pub fn consume(
    conn: &Connection,
    claim_token: &str,
    owner_name: &str,
    org_slug: Option<&str>,
) -> Result<TokenPair, GatewayError> {
    let claim_hash = token::sha256_hex(claim_token);

    let stored_hash: String = conn
        .query_row(
            "SELECT token_hash FROM claim_pending WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                GatewayError::BadRequest("no pending claim".into())
            }
            other => GatewayError::Db(other),
        })?;

    if !token::hashes_match(&claim_hash, &stored_hash) {
        return Err(GatewayError::Unauthorized("invalid claim token".into()));
    }

    let bearer = token::generate();
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM claim_pending WHERE id = 1", [])?;

    // SCOPE.md §13 Phase 4: the claim flow seeds the operator's org.
    // If the caller passes a non-default slug, allocate a fresh org;
    // otherwise stamp the owner into the seeded `default` org so
    // single-tenant gateways need not think about org ids at all.
    let org_id = match org_slug {
        Some(slug) if slug != orgs::DEFAULT_ORG_SLUG => {
            let existing = orgs::get_by_slug(&tx, slug)?;
            match existing {
                Some(o) => o.id,
                None => orgs::insert(&tx, slug, slug)?.id,
            }
        }
        _ => orgs::DEFAULT_ORG_ID,
    };

    tx.execute(
        "INSERT INTO users (org_id, name, role, token_hash, created_at)
         VALUES (?1, ?2, 'owner', ?3, unixepoch())",
        params![org_id, owner_name, bearer.hash],
    )?;
    tx.commit()?;

    Ok(bearer)
}
