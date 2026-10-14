use keyring::Entry;

const SERVICE_NAME: &str = "easy-writing";

pub fn set_secret(key_ref: &str, secret: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, key_ref).map_err(|e| e.to_string())?;
    entry.set_password(secret).map_err(|e| e.to_string())
}

pub fn get_secret(key_ref: &str) -> Result<String, String> {
    let entry = Entry::new(SERVICE_NAME, key_ref).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

pub fn delete_secret(key_ref: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, key_ref).map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())
}
