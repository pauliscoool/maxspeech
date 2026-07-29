use keyring::Entry;

const SERVICE: &str = "maxspeech";

pub fn set_secret(key: &str, value: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let entry = Entry::new(SERVICE, key)?;
    entry.set_password(value)?;
    Ok(())
}

pub fn get_secret(key: &str) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    let entry = Entry::new(SERVICE, key)?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
