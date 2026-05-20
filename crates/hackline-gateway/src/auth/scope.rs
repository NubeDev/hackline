//! `device_scope` / `tunnel_scope` enforcement for non-owner roles.
//! Called by handlers after the auth extractor has identified the
//! user. SCOPE.md §6.2 — `device_scope` is either `"*"` (every
//! device) or a JSON array of integer device ids.

use crate::db::users::User;
use crate::error::GatewayError;

/// Check that `user` may operate on `device_id`. Owner, admin,
/// support, and viewer roles see every device; `customer` is
/// restricted to its `device_scope` array. Adding a new role
/// requires deciding here whether it bypasses or honours the scope.
pub fn check_device(user: &User, device_id: i64) -> Result<(), GatewayError> {
    match user.role.as_str() {
        "owner" | "admin" | "support" | "viewer" => Ok(()),
        "customer" => {
            if user.device_scope == "*" {
                return Ok(());
            }
            let scope: serde_json::Value = serde_json::from_str(&user.device_scope)
                .map_err(|e| GatewayError::Config(format!("device_scope JSON: {e}")))?;
            let arr = scope.as_array().ok_or_else(|| {
                GatewayError::Config("device_scope must be `*` or a JSON array".into())
            })?;
            if arr.iter().any(|v| v.as_i64() == Some(device_id)) {
                Ok(())
            } else {
                Err(GatewayError::Unauthorized(format!(
                    "user {} not authorised for device {}",
                    user.id, device_id
                )))
            }
        }
        other => Err(GatewayError::Unauthorized(format!(
            "unknown role `{other}`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(role: &str, scope: &str) -> User {
        User {
            id: 1,
            org_id: 1,
            name: "u".into(),
            role: role.into(),
            device_scope: scope.into(),
            tunnel_scope: "*".into(),
            expires_at: None,
            created_at: 0,
            last_used_at: None,
        }
    }

    #[test]
    fn owner_bypasses() {
        assert!(check_device(&user("owner", "[]"), 7).is_ok());
    }

    #[test]
    fn customer_array_match() {
        assert!(check_device(&user("customer", "[1,2,3]"), 2).is_ok());
        assert!(check_device(&user("customer", "[1,2,3]"), 9).is_err());
        assert!(check_device(&user("customer", "*"), 9).is_ok());
    }
}
