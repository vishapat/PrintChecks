use argon2::Argon2;
use base64::{engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD}, Engine};
use chacha20poly1305::{aead::{Aead, KeyInit}, XChaCha20Poly1305, XNonce};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}, sync::Mutex, time::{SystemTime, UNIX_EPOCH}};
use tauri::{Manager, State};

#[derive(Clone, Serialize, Deserialize)]
struct KeySlot { salt: String, nonce: String, ciphertext: String }

#[derive(Clone, Serialize, Deserialize)]
struct VaultFileV2 { version: u8, data_nonce: String, ciphertext: String, passphrase_slot: KeySlot, recovery_slot: KeySlot }

// Kept only to upgrade vaults created by the first native-vault implementation.
#[derive(Deserialize)]
struct VaultFileV1 { salt: String, nonce: String, ciphertext: String }

#[derive(Serialize)]
struct VaultOpenResult { recovery_key: Option<String> }

type VaultContents = BTreeMap<String, String>;
type ActiveVault = (PathBuf, VaultContents, [u8; 32]);

struct VaultState(Mutex<Option<ActiveVault>>);

fn error(message: impl Into<String>) -> String { message.into() }
fn random<const N: usize>() -> [u8; N] { let mut value = [0u8; N]; OsRng.fill_bytes(&mut value); value }
fn derive(secret: &str, salt: &[u8]) -> Result<[u8; 32], String> {
  let mut key = [0u8; 32];
  Argon2::default().hash_password_into(secret.as_bytes(), salt, &mut key).map_err(|_| error("Unable to derive vault key"))?;
  Ok(key)
}
fn seal(key: &[u8; 32], plain: &[u8]) -> Result<(String, String), String> {
  let nonce = random::<24>();
  let ciphertext = XChaCha20Poly1305::new(key.into()).encrypt(XNonce::from_slice(&nonce), plain).map_err(|_| error("Unable to encrypt vault"))?;
  Ok((STANDARD.encode(nonce), STANDARD.encode(ciphertext)))
}
fn open(key: &[u8; 32], nonce: &str, ciphertext: &str, message: &str) -> Result<Vec<u8>, String> {
  let nonce = STANDARD.decode(nonce).map_err(|_| error("Invalid vault header"))?;
  let ciphertext = STANDARD.decode(ciphertext).map_err(|_| error("Invalid vault data"))?;
  XChaCha20Poly1305::new(key.into()).decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref()).map_err(|_| error(message))
}
fn create_slot(secret: &str, data_key: &[u8; 32]) -> Result<KeySlot, String> {
  let salt = random::<16>();
  let wrapping_key = derive(secret, &salt)?;
  let (nonce, ciphertext) = seal(&wrapping_key, data_key)?;
  Ok(KeySlot { salt: STANDARD.encode(salt), nonce, ciphertext })
}
fn unwrap_slot(secret: &str, slot: &KeySlot, message: &str) -> Result<[u8; 32], String> {
  let salt = STANDARD.decode(&slot.salt).map_err(|_| error("Invalid vault header"))?;
  let wrapping_key = derive(secret, &salt)?;
  let plain = open(&wrapping_key, &slot.nonce, &slot.ciphertext, message)?;
  plain.try_into().map_err(|_| error("Vault key is invalid"))
}
fn validate_passphrase(passphrase: &str, confirmation: &str) -> Result<(), String> {
  if passphrase.len() < 12 { return Err(error("Use a passphrase of at least 12 characters")); }
  if passphrase != confirmation { return Err(error("Passphrases do not match")); }
  Ok(())
}
fn new_recovery_key() -> String { format!("PC-RK-{}", URL_SAFE_NO_PAD.encode(random::<32>())) }
fn make_v2(data: &BTreeMap<String, String>, passphrase: &str) -> Result<(VaultFileV2, [u8; 32], String), String> {
  let data_key = random::<32>();
  let recovery_key = new_recovery_key();
  let plain = serde_json::to_vec(data).map_err(|_| error("Unable to encode vault"))?;
  let (data_nonce, ciphertext) = seal(&data_key, &plain)?;
  Ok((VaultFileV2 { version: 2, data_nonce, ciphertext, passphrase_slot: create_slot(passphrase, &data_key)?, recovery_slot: create_slot(&recovery_key, &data_key)? }, data_key, recovery_key))
}
fn backup_path(path: &Path) -> PathBuf { path.with_file_name("printchecks.vault.previous") }
fn restore_interrupted_write(path: &Path) -> Result<(), String> {
  let backup = backup_path(path);
  if !path.exists() && backup.exists() {
    fs::rename(backup, path).map_err(|_| error("Could not restore the previous vault"))?;
  }
  Ok(())
}
fn write_vault(path: &Path, vault: &VaultFileV2) -> Result<(), String> {
  let bytes = serde_json::to_vec_pretty(vault).map_err(|_| error("Unable to encode vault"))?;
  let temporary = path.with_file_name("printchecks.vault.tmp");
  fs::write(&temporary, bytes).map_err(|_| error("Unable to save vault"))?;
  if !path.exists() {
    fs::copy(&temporary, path).map_err(|_| error("Unable to finalize vault save"))?;
    fs::remove_file(&temporary).map_err(|_| error("Unable to finalize vault save"))?;
    return Ok(());
  }
  let backup = backup_path(path);
  if backup.exists() { fs::remove_file(&backup).map_err(|_| error("Unable to prepare vault backup"))?; }
  fs::rename(path, &backup).map_err(|_| error("Unable to preserve the previous vault"))?;
  if fs::rename(&temporary, path).is_err() {
    let _ = fs::rename(&backup, path);
    let _ = fs::remove_file(&temporary);
    return Err(error("Unable to finalize vault save"));
  }
  fs::remove_file(backup).map_err(|_| error("Vault was saved, but its temporary backup could not be removed"))
}
fn decode_data(data_key: &[u8; 32], nonce: &str, ciphertext: &str) -> Result<BTreeMap<String, String>, String> {
  serde_json::from_slice(&open(data_key, nonce, ciphertext, "Incorrect passphrase, recovery key, or corrupted vault")?).map_err(|_| error("Vault contents are invalid"))
}
fn default_path(app: &tauri::AppHandle) -> Result<PathBuf, String> { app.path().document_dir().map_err(|_| error("Could not find Documents folder")).map(|p| p.join("PrintChecks").join("printchecks.vault")) }

#[tauri::command]
fn vault_exists(app: tauri::AppHandle) -> Result<bool, String> {
  let path = default_path(&app)?;
  restore_interrupted_write(&path)?;
  Ok(path.exists())
}

#[tauri::command]
fn create_vault(app: tauri::AppHandle, passphrase: String, confirmation: String, initial_data: Option<BTreeMap<String, String>>, state: State<VaultState>) -> Result<String, String> {
  validate_passphrase(&passphrase, &confirmation)?;
  let path = default_path(&app)?;
  if path.exists() { return Err(error("A vault already exists. Unlock it, use the recovery key, or start a new vault.")); }
  fs::create_dir_all(path.parent().ok_or_else(|| error("Invalid vault path"))?).map_err(|_| error("Could not create vault folder"))?;
  let data = initial_data.unwrap_or_default();
  let (vault, data_key, recovery_key) = make_v2(&data, &passphrase)?;
  write_vault(&path, &vault)?;
  *state.0.lock().map_err(|_| error("Vault is busy"))? = Some((path, data, data_key));
  Ok(recovery_key)
}

#[tauri::command]
fn unlock_vault(app: tauri::AppHandle, passphrase: String, state: State<VaultState>) -> Result<VaultOpenResult, String> {
  let path = default_path(&app)?;
  restore_interrupted_write(&path)?;
  let bytes = fs::read(&path).map_err(|_| error("No vault exists yet. Create one instead."))?;
  let version = serde_json::from_slice::<serde_json::Value>(&bytes).ok().and_then(|v| v.get("version").and_then(|v| v.as_u64())).ok_or_else(|| error("Selected file is not a PrintChecks vault"))?;
  let (data, data_key, recovery_key) = if version == 1 {
    let legacy: VaultFileV1 = serde_json::from_slice(&bytes).map_err(|_| error("Selected file is not a PrintChecks vault"))?;
    let salt = STANDARD.decode(legacy.salt).map_err(|_| error("Invalid vault header"))?;
    let legacy_key = derive(&passphrase, &salt)?;
    let data: BTreeMap<String, String> = serde_json::from_slice(&open(&legacy_key, &legacy.nonce, &legacy.ciphertext, "Incorrect passphrase or corrupted vault")?).map_err(|_| error("Vault contents are invalid"))?;
    let (upgraded, data_key, recovery_key) = make_v2(&data, &passphrase)?;
    write_vault(&path, &upgraded)?;
    (data, data_key, Some(recovery_key))
  } else if version == 2 {
    let vault: VaultFileV2 = serde_json::from_slice(&bytes).map_err(|_| error("Selected file is not a PrintChecks vault"))?;
    let data_key = unwrap_slot(&passphrase, &vault.passphrase_slot, "Incorrect passphrase or corrupted vault")?;
    (decode_data(&data_key, &vault.data_nonce, &vault.ciphertext)?, data_key, None)
  } else { return Err(error("Unsupported vault version")); };
  *state.0.lock().map_err(|_| error("Vault is busy"))? = Some((path, data, data_key));
  Ok(VaultOpenResult { recovery_key })
}

#[tauri::command]
fn reset_passphrase_with_recovery(app: tauri::AppHandle, recovery_key: String, passphrase: String, confirmation: String, state: State<VaultState>) -> Result<(), String> {
  validate_passphrase(&passphrase, &confirmation)?;
  let path = default_path(&app)?;
  restore_interrupted_write(&path)?;
  let mut vault: VaultFileV2 = serde_json::from_slice(&fs::read(&path).map_err(|_| error("No vault exists yet. Create one instead."))?).map_err(|_| error("Selected file is not a recovery-enabled PrintChecks vault"))?;
  if vault.version != 2 { return Err(error("Unlock this older vault with its passphrase before using recovery.")); }
  let data_key = unwrap_slot(&recovery_key, &vault.recovery_slot, "Incorrect recovery key or corrupted vault")?;
  let data = decode_data(&data_key, &vault.data_nonce, &vault.ciphertext)?;
  vault.passphrase_slot = create_slot(&passphrase, &data_key)?;
  write_vault(&path, &vault)?;
  *state.0.lock().map_err(|_| error("Vault is busy"))? = Some((path, data, data_key));
  Ok(())
}

#[tauri::command]
fn start_new_vault(app: tauri::AppHandle, passphrase: String, confirmation: String, state: State<VaultState>) -> Result<String, String> {
  validate_passphrase(&passphrase, &confirmation)?;
  let path = default_path(&app)?;
  restore_interrupted_write(&path)?;
  if path.exists() {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| error("Could not create archive name"))?.as_secs();
    let archive = path.with_file_name(format!("printchecks.unrecoverable-{}.vault", timestamp));
    fs::rename(&path, archive).map_err(|_| error("Could not preserve the existing vault"))?;
  }
  let data = BTreeMap::new();
  let (vault, data_key, recovery_key) = make_v2(&data, &passphrase)?;
  write_vault(&path, &vault)?;
  *state.0.lock().map_err(|_| error("Vault is busy"))? = Some((path, data, data_key));
  Ok(recovery_key)
}

#[tauri::command]
fn vault_get(key: String, state: State<VaultState>) -> Result<Option<String>, String> { Ok(state.0.lock().map_err(|_| error("Vault is busy"))?.as_ref().ok_or_else(|| error("Vault is locked"))?.1.get(&key).cloned()) }

#[tauri::command]
fn vault_set(key: String, value: String, state: State<VaultState>) -> Result<(), String> {
  let mut guard = state.0.lock().map_err(|_| error("Vault is busy"))?;
  let (path, data, data_key) = guard.as_mut().ok_or_else(|| error("Vault is locked"))?;
  data.insert(key, value);
  let mut vault: VaultFileV2 = serde_json::from_slice(&fs::read(&*path).map_err(|_| error("Vault file could not be read"))?).map_err(|_| error("Vault file is invalid"))?;
  let plain = serde_json::to_vec(data).map_err(|_| error("Unable to encode vault"))?;
  let (nonce, ciphertext) = seal(data_key, &plain)?;
  vault.data_nonce = nonce; vault.ciphertext = ciphertext;
  write_vault(path, &vault)
}

#[tauri::command]
fn vault_lock(state: State<VaultState>) -> Result<(), String> {
  *state.0.lock().map_err(|_| error("Vault is busy"))? = None;
  Ok(())
}
fn main() { tauri::Builder::default().plugin(tauri_plugin_dialog::init()).manage(VaultState(Mutex::new(None))).invoke_handler(tauri::generate_handler![vault_exists, create_vault, unlock_vault, reset_passphrase_with_recovery, start_new_vault, vault_get, vault_set, vault_lock]).run(tauri::generate_context!()).expect("error while running PrintChecks"); }

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn vault_round_trip_works_with_passphrase_and_recovery_key() {
    let mut data = BTreeMap::new(); data.insert("bankAccounts".into(), "[\"sensitive\"]".into());
    let (vault, data_key, recovery_key) = make_v2(&data, "a long enough passphrase").unwrap();
    let from_passphrase = unwrap_slot("a long enough passphrase", &vault.passphrase_slot, "bad").unwrap();
    let from_recovery = unwrap_slot(&recovery_key, &vault.recovery_slot, "bad").unwrap();
    assert_eq!(data_key, from_passphrase); assert_eq!(data_key, from_recovery);
    assert_eq!(decode_data(&data_key, &vault.data_nonce, &vault.ciphertext).unwrap().get("bankAccounts"), data.get("bankAccounts"));
  }
  #[test]
  fn wrong_recovery_key_cannot_open_key_slot() {
    let (vault, _, _) = make_v2(&BTreeMap::new(), "a long enough passphrase").unwrap();
    assert!(unwrap_slot("PC-RK-not-the-key", &vault.recovery_slot, "bad recovery").is_err());
  }
  #[test]
  fn passphrase_validation_requires_length_and_confirmation() {
    assert!(validate_passphrase("short", "short").is_err());
    assert!(validate_passphrase("a long enough passphrase", "different passphrase").is_err());
    assert!(validate_passphrase("a long enough passphrase", "a long enough passphrase").is_ok());
  }
  #[test]
  fn interrupted_vault_write_restores_the_previous_file() {
    let directory = std::env::current_dir().unwrap().join("target").join(format!("printchecks-vault-test-{}", URL_SAFE_NO_PAD.encode(random::<12>())));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("printchecks.vault");
    let (vault, _, _) = make_v2(&BTreeMap::new(), "a long enough passphrase").unwrap();
    write_vault(&path, &vault).unwrap();
    write_vault(&path, &vault).unwrap();
    assert!(path.exists());
    assert!(!backup_path(&path).exists());
    fs::rename(&path, backup_path(&path)).unwrap();
    restore_interrupted_write(&path).unwrap();
    assert!(path.exists());
    assert!(!backup_path(&path).exists());
    fs::remove_dir_all(directory).unwrap();
  }
}
