use keyring::{Entry, Error};

const SERVICE: &str = "ZhiForge";
const LEGACY_ACCOUNT: &str = "default-provider-api-key";

fn entry(account: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, account).map_err(|error| format!("credential store unavailable: {error}"))
}

fn provider_account(credential_id: &str) -> Result<String, String> {
    let id = credential_id.trim();
    if id.is_empty() {
        return Err("credential id is empty".into());
    }
    if id.len() > 200 {
        return Err("credential id is too long".into());
    }
    Ok(format!("provider:{id}"))
}

fn load_account(account: &str) -> Result<String, String> {
    match entry(account)?.get_password() {
        Ok(password) => Ok(password),
        Err(Error::NoEntry) => Ok(String::new()),
        Err(error) => Err(format!("failed to read API key from credential store: {error}")),
    }
}

fn save_account(account: &str, api_key: &str) -> Result<(), String> {
    let entry = entry(account)?;
    if api_key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(format!("failed to clear API key from credential store: {error}")),
        }
    } else {
        entry
            .set_password(api_key)
            .map_err(|error| format!("failed to save API key to Windows Credential Manager: {error}"))?;
        let stored = entry
            .get_password()
            .map_err(|error| format!("API key was written but could not be verified: {error}"))?;
        if stored != api_key {
            return Err("API key verification failed after saving to Windows Credential Manager".into());
        }
        Ok(())
    }
}

pub fn load() -> Result<String, String> {
    load_account(LEGACY_ACCOUNT)
}

pub fn save(api_key: &str) -> Result<(), String> {
    save_account(LEGACY_ACCOUNT, api_key)
}

pub fn load_provider(credential_id: &str) -> Result<String, String> {
    load_account(&provider_account(credential_id)?)
}

pub fn save_provider(credential_id: &str, api_key: &str) -> Result<(), String> {
    save_account(&provider_account(credential_id)?, api_key)
}

pub fn migrate_legacy_to_provider(credential_id: &str) -> Result<bool, String> {
    let legacy = load()?;
    if legacy.is_empty() {
        return Ok(false);
    }

    save_provider(credential_id, &legacy)?;
    save("")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_account_is_namespaced() {
        assert_eq!(provider_account("openrouter-main").unwrap(), "provider:openrouter-main");
        assert!(provider_account("   ").is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "touches Windows Credential Manager with a temporary self-test credential"]
    fn windows_native_credential_round_trip() {
        let credential_id = format!("self-test-{}", std::process::id());
        let expected = "zhiforge-keyring-self-test";
        let save_result = save_provider(&credential_id, expected);
        let loaded_result = load_provider(&credential_id);
        let cleanup_result = save_provider(&credential_id, "");

        assert!(save_result.is_ok(), "save failed: {save_result:?}");
        assert_eq!(loaded_result.unwrap(), expected);
        assert!(cleanup_result.is_ok(), "cleanup failed: {cleanup_result:?}");
    }
}
