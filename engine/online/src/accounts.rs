//! Accounts and their characters, one file each (`<dir>/<account>.ron`): a salted password
//! hash and the stored character. A new account name logs in by making the account.

use c3_game::PlayerSave;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountError {
    BadName,
    BadPassword,
    WrongPassword,
    /// The account file couldn't be read or written.
    Storage(String),
}

impl fmt::Display for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::BadName => write!(f, "Account names are 3 to 16 letters, digits or _."),
            Self::BadPassword => write!(f, "Passwords are 1 to 64 characters."),
            Self::WrongPassword => write!(f, "Wrong password."),
            Self::Storage(why) => write!(f, "The account couldn't be read or saved: {why}"),
        }
    }
}

impl std::error::Error for AccountError {}

#[derive(Serialize, Deserialize)]
struct AccountFile {
    salt: String,
    /// SHA-256 of salt and password, in hex.
    password: String,
    character: PlayerSave,
}

fn hash(salt: &str, password: &str) -> String {
    let digest = Sha256::digest(format!("{salt}:{password}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A fresh salt (the account name and the time it was made).
fn new_salt(account: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    hash(&nanos.to_string(), account)[..16].to_owned()
}

fn valid_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The accounts kept in a folder.
pub struct Accounts {
    dir: PathBuf,
}

impl Accounts {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self, account: &str) -> PathBuf {
        self.dir
            .join(format!("{}.ron", account.to_ascii_lowercase()))
    }

    fn read(file: &Path) -> Result<AccountFile, AccountError> {
        let text =
            std::fs::read_to_string(file).map_err(|e| AccountError::Storage(e.to_string()))?;
        ron::from_str(&text).map_err(|e| AccountError::Storage(e.to_string()))
    }

    fn write(&self, account: &str, data: &AccountFile) -> Result<(), AccountError> {
        let storage = |e: &dyn fmt::Display| AccountError::Storage(e.to_string());
        let text = ron::ser::to_string_pretty(data, ron::ser::PrettyConfig::default())
            .map_err(|e| storage(&e))?;
        std::fs::create_dir_all(&self.dir).map_err(|e| storage(&e))?;
        let file = self.file(account);
        let tmp = file.with_extension("tmp");
        std::fs::write(&tmp, text).map_err(|e| storage(&e))?;
        std::fs::rename(&tmp, &file).map_err(|e| storage(&e))
    }

    /// Logs in to `account`: its stored character, or `new_character()` for a new account
    /// (which is made at once).
    pub fn login(
        &self,
        account: &str,
        password: &str,
        new_character: impl FnOnce(&str) -> PlayerSave,
    ) -> Result<PlayerSave, AccountError> {
        if !valid_name(account) {
            return Err(AccountError::BadName);
        }
        if password.is_empty() || password.chars().count() > 64 {
            return Err(AccountError::BadPassword);
        }
        let file = self.file(account);
        if file.exists() {
            let data = Self::read(&file)?;
            if hash(&data.salt, password) != data.password {
                return Err(AccountError::WrongPassword);
            }
            return Ok(data.character);
        }
        let salt = new_salt(account);
        let data = AccountFile {
            password: hash(&salt, password),
            salt,
            character: new_character(account),
        };
        self.write(account, &data)?;
        Ok(data.character)
    }

    /// Stores `character` as `account`'s (the account must exist).
    pub fn save(&self, account: &str, character: PlayerSave) -> Result<(), AccountError> {
        let mut data = Self::read(&self.file(account))?;
        data.character = character;
        self.write(account, &data)
    }
}
