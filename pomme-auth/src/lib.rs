//! Microsoft account sign-in for Minecraft: the device-code sign-in's token
//! is exchanged through Xbox Live for a Minecraft access token, and the
//! account and its refresh token are kept in a [`SecretStore`].
//!
//! Every call takes the HTTP client to use, so the client can pass one that
//! trusts Android's CA store.

mod device_code;
mod store;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub use device_code::{DeviceCode, device_code_sign_in};
use serde::{Deserialize, Serialize};
#[cfg(feature = "keyring")]
pub use store::KeyringStore;
pub use store::{FileStore, SecretStore};

const CLIENT_ID: &str = "b2dd2c0f-8a09-4549-9d76-ab43b8572695";
const SCOPE: &str = "XboxLive.signin offline_access";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";

const ACCOUNTS_KEY: &str = "minecraft-accounts";
const REFRESH_KEY: &str = "minecraft-refresh-tokens";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthAccount {
    pub username: String,
    #[serde(rename = "profileId", alias = "uuid")]
    pub uuid: String,
    #[serde(alias = "access_token")]
    pub access_token: String,
    #[serde(alias = "expires_at")]
    pub expires_at: u64,
    #[serde(default, alias = "is_demo")]
    pub is_demo: bool,
    #[serde(default)]
    #[serde(alias = "xbox_gamertag")]
    pub xbox_gamertag: String,
    #[serde(default, alias = "account_id")]
    pub account_id: String,
    #[serde(default)]
    pub xuid: String,
    #[serde(default, rename = "profilePicURL", alias = "profile_pic_url")]
    pub profile_pic_url: Option<String>,
}

#[derive(Deserialize)]
struct MsaTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct XblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Deserialize)]
struct XblDisplayClaims {
    xui: Vec<XblXui>,
}

#[derive(Deserialize)]
struct XblXui {
    uhs: String,
    #[serde(default, rename = "gt")]
    _gt: Option<String>,
    #[serde(default, rename = "xid", alias = "id")]
    xid: Option<String>,
}

#[derive(Deserialize)]
struct McAuthResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct McProfileResponse {
    id: String,
    name: String,
}

/// Error message including the source chain (`reqwest` hides the detail there).
pub fn error_chain(e: impl std::error::Error) -> String {
    let mut msg = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        let text = s.to_string();
        if !msg.contains(&text) {
            msg.push_str(": ");
            msg.push_str(&text);
        }
        source = s.source();
    }
    msg
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_json<T: serde::de::DeserializeOwned + Default>(store: &dyn SecretStore, key: &str) -> T {
    store
        .read(key)
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn write_json(store: &dyn SecretStore, key: &str, value: &impl Serialize) {
    if let Ok(json) = serde_json::to_string(value) {
        store.write(key, &json);
    }
}

pub fn get_all_accounts(store: &dyn SecretStore) -> Vec<AuthAccount> {
    read_json(store, ACCOUNTS_KEY)
}

fn refresh_tokens(store: &dyn SecretStore) -> HashMap<String, String> {
    read_json(store, REFRESH_KEY)
}

/// The stored account, while its access token is still valid.
fn account_key(account: &AuthAccount) -> &str {
    if account.account_id.is_empty() {
        &account.uuid
    } else {
        &account.account_id
    }
}

pub fn try_restore(store: &dyn SecretStore, key: &str) -> Option<AuthAccount> {
    get_all_accounts(store)
        .into_iter()
        .find(|a| (a.uuid == key || account_key(a) == key) && a.expires_at > unix_now())
}

pub async fn try_refresh(
    http: &reqwest::Client,
    store: &dyn SecretStore,
    uuid: &str,
) -> Option<AuthAccount> {
    let refresh_token = refresh_tokens(store).remove(uuid)?;
    refresh_msa_token(http, store, &refresh_token).await.ok()
}

pub async fn try_restore_or_refresh(
    http: &reqwest::Client,
    store: &dyn SecretStore,
    uuid: &str,
) -> Option<AuthAccount> {
    match try_restore(store, uuid) {
        Some(account) => Some(account),
        None => try_refresh(http, store, uuid).await,
    }
}

pub fn remove_account(store: &dyn SecretStore, uuid: &str) {
    let mut accounts = get_all_accounts(store);
    let removed_key = accounts
        .iter()
        .find(|a| a.uuid == uuid || account_key(a) == uuid)
        .map(|a| account_key(a).to_owned());
    accounts.retain(|a| a.uuid != uuid && account_key(a) != uuid);
    write_json(store, ACCOUNTS_KEY, &accounts);
    let mut tokens = refresh_tokens(store);
    tokens.remove(uuid);
    if let Some(key) = removed_key {
        tokens.remove(&key);
    }
    write_json(store, REFRESH_KEY, &tokens);
}

fn save_account(store: &dyn SecretStore, account: &AuthAccount, refresh_token: Option<&str>) {
    let mut accounts = get_all_accounts(store);
    let key = account_key(account).to_owned();
    accounts.retain(|a| account_key(a) != key);
    accounts.push(account.clone());
    write_json(store, ACCOUNTS_KEY, &accounts);
    if let Some(token) = refresh_token {
        let mut tokens = refresh_tokens(store);
        tokens.insert(key, token.to_owned());
        write_json(store, REFRESH_KEY, &tokens);
    }
}

async fn refresh_msa_token(
    http: &reqwest::Client,
    store: &dyn SecretStore,
    refresh_token: &str,
) -> Result<AuthAccount, String> {
    let msa: MsaTokenResponse = http
        .post(TOKEN_URL)
        .form(&[
            ("client_id", CLIENT_ID),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
            ("scope", SCOPE),
        ])
        .send()
        .await
        .map_err(|e| format!("Refresh failed: {}", crate::error_chain(e)))?
        .json()
        .await
        .map_err(|e| format!("Refresh parse failed: {}", crate::error_chain(e)))?;
    finish_msa_exchange(http, store, &msa).await
}

/// Exchanges a fresh Microsoft token for a Minecraft account and stores it.
async fn finish_msa_exchange(
    http: &reqwest::Client,
    store: &dyn SecretStore,
    msa: &MsaTokenResponse,
) -> Result<AuthAccount, String> {
    let account = exchange_msa_to_minecraft(http, &msa.access_token).await?;
    save_account(store, &account, msa.refresh_token.as_deref());
    Ok(account)
}

async fn exchange_msa_to_minecraft(
    http: &reqwest::Client,
    msa_token: &str,
) -> Result<AuthAccount, String> {
    let xbl: XblResponse = http
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={msa_token}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
        .send()
        .await
        .map_err(|e| format!("Xbox Live auth failed: {}", crate::error_chain(e)))?
        .json()
        .await
        .map_err(|e| format!("Xbox Live parse failed: {}", crate::error_chain(e)))?;

    let xui = xbl
        .display_claims
        .xui
        .first()
        .ok_or("No user hash in XBL response")?;
    let user_hash = xui.uhs.clone();

    // Amethyst obtains the gamerpic and canonical gamertag from the Xbox
    // profile API using an XSTS token scoped to xboxlive.com.
    let xbox_xsts = http
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&serde_json::json!({"Properties":{"SandboxId":"RETAIL","UserTokens":[&xbl.token]},"RelyingParty":"http://xboxlive.com","TokenType":"JWT"}))
        .send().await.map_err(|e| format!("Xbox profile XSTS failed: {}", crate::error_chain(e)))?;
    let xbox_xsts: XblResponse = xbox_xsts
        .error_for_status()
        .map_err(|e| format!("Xbox profile XSTS refused: {}", crate::error_chain(e)))?
        .json()
        .await
        .map_err(|e| format!("Xbox profile XSTS parse failed: {}", crate::error_chain(e)))?;
    #[derive(Deserialize)]
    struct XboxProfileResponse {
        #[serde(rename = "profileUsers")]
        profile_users: Vec<XboxProfileUser>,
    }
    #[derive(Deserialize)]
    struct XboxProfileUser {
        settings: Vec<XboxProfileSetting>,
    }
    #[derive(Deserialize)]
    struct XboxProfileSetting {
        value: String,
    }
    let xuid = xbox_xsts
        .display_claims
        .xui
        .first()
        .and_then(|claim| claim.xid.clone())
        .ok_or("Xbox XSTS response had no XUID")?;
    let xbox_profile: XboxProfileResponse = http.get("https://profile.xboxlive.com/users/me/profile/settings?settings=PublicGamerpic,Gamertag")
        .header("x-xbl-contract-version", "2")
        .header("Authorization", format!("XBL3.0 x={};{}", user_hash, xbox_xsts.token))
        .send().await.map_err(|e| format!("Xbox profile fetch failed: {}", crate::error_chain(e)))?
        .error_for_status().map_err(|e| format!("Xbox profile refused: {}", crate::error_chain(e)))?
        .json().await.map_err(|e| format!("Xbox profile parse failed: {}", crate::error_chain(e)))?;
    let settings = &xbox_profile
        .profile_users
        .first()
        .ok_or("Xbox profile response had no user")?
        .settings;
    let gamerpic = settings.first().map(|s| format!("{}&h=120&w=120", s.value));
    let gamertag = settings
        .get(1)
        .map(|s| s.value.clone())
        .ok_or("Xbox profile response had no gamertag")?;

    let xsts = http
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [&xbl.token],
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))
        .send()
        .await
        .map_err(|e| format!("XSTS auth failed: {}", crate::error_chain(e)))?;
    let xsts_ok = xsts.status().is_success();
    let xsts_body = xsts
        .text()
        .await
        .map_err(|e| format!("XSTS read failed: {}", crate::error_chain(e)))?;
    if !xsts_ok {
        return Err(xsts_error(&xsts_body));
    }
    let xsts: XblResponse = serde_json::from_str(&xsts_body)
        .map_err(|e| format!("XSTS parse failed: {}", crate::error_chain(e)))?;

    let mc: McAuthResponse = http
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&serde_json::json!({
            "identityToken": format!("XBL3.0 x={user_hash};{}", xsts.token),
        }))
        .send()
        .await
        .map_err(|e| format!("MC auth failed: {}", crate::error_chain(e)))?
        .json()
        .await
        .map_err(|e| format!("MC auth parse failed: {}", crate::error_chain(e)))?;

    let profile = http
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&mc.access_token)
        .send()
        .await
        .map_err(|e| format!("Profile fetch failed: {}", crate::error_chain(e)))?;

    if profile.status() == reqwest::StatusCode::NOT_FOUND {
        // Match Amethyst: Xbox-authenticated accounts without a Java profile
        // receive a demo identity based on their Xbox gamertag.
        let username = demo_player_name(&gamertag);
        return Ok(AuthAccount {
            uuid: "00000000-0000-0000-0000-000000000000".into(),
            username,
            access_token: String::new(),
            expires_at: unix_now() + 86400,
            is_demo: true,
            xbox_gamertag: gamertag,
            account_id: xuid.clone(),
            xuid,
            profile_pic_url: gamerpic,
        });
    }
    if !profile.status().is_success() {
        return Err(format!(
            "Minecraft profile lookup failed: HTTP {}",
            profile.status()
        ));
    }
    let profile: McProfileResponse = profile
        .json()
        .await
        .map_err(|e| format!("Profile parse failed: {}", crate::error_chain(e)))?;

    Ok(AuthAccount {
        username: profile.name,
        uuid: profile.id,
        access_token: mc.access_token,
        expires_at: unix_now() + 86400,
        is_demo: false,
        xbox_gamertag: gamertag,
        account_id: xuid.clone(),
        xuid,
        profile_pic_url: None,
    })
}

/// Builds a Java-compatible offline name for an Xbox account without a Java
/// profile. Java player names are 3–16 ASCII letters, digits, or underscores;
/// Xbox gamertags may contain spaces and punctuation and can be longer.
fn demo_player_name(gamertag: &str) -> String {
    let safe: String = gamertag
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(11)
        .collect();
    if safe.is_empty() {
        "Demo_Player".to_owned()
    } else {
        format!("Demo_{safe}")
    }
}

/// Why XSTS refused the account, from its `XErr` code.
fn xsts_error(body: &str) -> String {
    #[derive(Deserialize)]
    struct XstsError {
        #[serde(rename = "XErr")]
        code: u64,
    }
    let reason = match serde_json::from_str::<XstsError>(body).map(|e| e.code) {
        Ok(2148916233) => "This Microsoft account has no Xbox profile; create one at xbox.com",
        Ok(2148916235) => "Xbox Live isn't available in this account's country",
        Ok(2148916236 | 2148916237) => "This account needs adult verification on xbox.com",
        Ok(2148916238) => "This is a child account; an adult must add it to a Microsoft family",
        _ => return format!("XSTS auth refused: {body}"),
    };
    reason.into()
}

#[cfg(test)]
mod tests {
    use super::{
        AuthAccount, FileStore, demo_player_name, get_all_accounts, remove_account, save_account,
        xsts_error,
    };

    #[test]
    fn demo_names_follow_java_player_name_rules() {
        assert_eq!(demo_player_name("caselq"), "Demo_caselq");
        assert_eq!(demo_player_name("Player Name!"), "Demo_PlayerName");
        assert_eq!(demo_player_name("abcdefghijklmnop"), "Demo_abcdefghijk");
        assert_eq!(demo_player_name("!!!"), "Demo_Player");
        for name in [
            "Demo_caselq",
            "Demo_PlayerName",
            "Demo_abcdefghijk",
            "Demo_Player",
        ] {
            assert!((3..=16).contains(&name.len()));
            assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        }
    }

    #[test]
    fn xsts_errors_read_as_reasons() {
        assert!(xsts_error(r#"{"XErr":2148916233,"Message":""}"#).contains("no Xbox profile"));
        assert!(xsts_error(r#"{"XErr":2148916238}"#).contains("child account"));
        assert!(xsts_error("not json").starts_with("XSTS auth refused"));
    }

    #[test]
    fn accounts_save_replace_and_remove() {
        let dir = std::env::temp_dir().join(format!("pomme-auth-accounts-{}", std::process::id()));
        let store = FileStore {
            path: dir.join("accounts.json"),
        };
        let account = |token: &str| AuthAccount {
            username: "Tester".into(),
            uuid: "abc".into(),
            access_token: token.into(),
            expires_at: 0,
            is_demo: false,
            xbox_gamertag: String::new(),
            account_id: String::new(),
            xuid: String::new(),
            profile_pic_url: None,
        };
        save_account(&store, &account("one"), Some("refresh"));
        save_account(&store, &account("two"), None);
        assert_eq!(get_all_accounts(&store), [account("two")]);
        remove_account(&store, "abc");
        assert!(get_all_accounts(&store).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
