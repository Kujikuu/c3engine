//! Accounts and their characters, one file each (`<dir>/<account>.ron`): a salted password
//! hash and the stored characters. A new account name logs in by making the account.

use c3_game::{PlayerLook, PlayerSave};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountError {
    BadName,
    BadPassword,
    WrongPassword,
    BadCharacterName,
    /// The account has no character of that name.
    NoCharacter,
    /// Another character has the name.
    NameTaken,
    /// The account has as many characters as it may.
    TooManyCharacters,
    /// The account file couldn't be read or written.
    Storage(String),
}

impl fmt::Display for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::BadName => write!(f, "Account names are 3 to 16 letters, digits or _."),
            Self::BadPassword => write!(f, "Passwords are 1 to 64 characters."),
            Self::WrongPassword => write!(f, "Wrong password."),
            Self::BadCharacterName => {
                write!(f, "Character names are 3 to 16 letters, digits or _.")
            }
            Self::NoCharacter => write!(f, "The account has no such character."),
            Self::NameTaken => write!(f, "That name is taken."),
            Self::TooManyCharacters => write!(f, "The account can't have more characters."),
            Self::Storage(why) => write!(f, "The account couldn't be read or saved: {why}"),
        }
    }
}

impl std::error::Error for AccountError {}

/// Which character a login plays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Enter {
    /// The account's first character, made (named after the account) when it has none.
    First,
    /// The account's character of this name.
    Existing(String),
    /// A new character of the account: its name, look and profession (0: none).
    New {
        name: String,
        look: PlayerLook,
        #[serde(default)]
        profession: u32,
    },
}

#[derive(Serialize, Deserialize)]
struct AccountFile {
    salt: String,
    /// SHA-256 of salt and password, in hex.
    password: String,
    #[serde(default)]
    characters: Vec<PlayerSave>,
    /// The one character of accounts made before they could have several.
    #[serde(default, skip_serializing)]
    character: Option<PlayerSave>,
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
        let mut data: AccountFile =
            ron::from_str(&text).map_err(|e| AccountError::Storage(e.to_string()))?;
        data.characters.extend(data.character.take());
        Ok(data)
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

    /// `account`'s file, checked against `password`; a new account is made (without
    /// characters).
    fn open(&self, account: &str, password: &str) -> Result<AccountFile, AccountError> {
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
            return Ok(data);
        }
        let salt = new_salt(account);
        let data = AccountFile {
            password: hash(&salt, password),
            salt,
            characters: Vec::new(),
            character: None,
        };
        self.write(account, &data)?;
        Ok(data)
    }

    /// The characters of `account` (made, empty, when it is new).
    pub fn characters(
        &self,
        account: &str,
        password: &str,
    ) -> Result<Vec<PlayerSave>, AccountError> {
        Ok(self.open(account, password)?.characters)
    }

    /// Whether any account has a character called `name`.
    fn name_taken(&self, name: &str) -> bool {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return false;
        };
        (entries.flatten())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
            .filter_map(|e| Self::read(&e.path()).ok())
            .any(|a| {
                a.characters
                    .iter()
                    .any(|c| c.name.eq_ignore_ascii_case(name))
            })
    }

    /// Logs in to `account` playing the character `enter` names; a new one starts as
    /// `new_character(name)`, and an account has at most `most` characters.
    pub fn login(
        &self,
        account: &str,
        password: &str,
        enter: &Enter,
        most: usize,
        new_character: impl FnOnce(&str) -> PlayerSave,
    ) -> Result<PlayerSave, AccountError> {
        let mut data = self.open(account, password)?;
        let (name, look, profession) = match enter {
            Enter::First => match data.characters.first() {
                Some(first) => return Ok(first.clone()),
                None => (account, PlayerLook::default(), 0),
            },
            Enter::Existing(name) => {
                return (data.characters.iter())
                    .find(|c| c.name.eq_ignore_ascii_case(name))
                    .cloned()
                    .ok_or(AccountError::NoCharacter);
            }
            Enter::New {
                name,
                look,
                profession,
            } => {
                if !valid_name(name) {
                    return Err(AccountError::BadCharacterName);
                }
                if data.characters.len() >= most.max(1) {
                    return Err(AccountError::TooManyCharacters);
                }
                if self.name_taken(name) {
                    return Err(AccountError::NameTaken);
                }
                (name.as_str(), *look, *profession)
            }
        };
        let mut character = new_character(name);
        character.name = name.to_owned();
        character.look = look;
        // The one picked, else the game's default.
        if profession != 0 {
            character.profession = profession;
        }
        data.characters.push(character.clone());
        self.write(account, &data)?;
        Ok(character)
    }

    /// Stores `character` as `account`'s (by its name; the account must exist).
    pub fn save(&self, account: &str, character: PlayerSave) -> Result<(), AccountError> {
        let mut data = Self::read(&self.file(account))?;
        let stored =
            (data.characters.iter_mut()).find(|c| c.name.eq_ignore_ascii_case(&character.name));
        match stored {
            Some(stored) => *stored = character,
            None => data.characters.push(character),
        }
        self.write(account, &data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts(name: &str) -> Accounts {
        let dir = std::env::temp_dir().join(format!("c3-accounts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Accounts::new(dir)
    }

    fn new(name: &str) -> Enter {
        Enter::New {
            name: name.into(),
            look: PlayerLook {
                body: 2,
                hair: 7,
                face: 0,
            },
            profession: 10,
        }
    }

    #[test]
    fn accounts_list_make_and_enter_characters() {
        let a = accounts("chars");
        let start = |name: &str| PlayerSave::new(name, (1, 1));
        assert!(a.characters("ann", "pw").unwrap().is_empty());
        let made = a.login("ann", "pw", &new("Hero"), 2, start).unwrap();
        assert_eq!((made.name.as_str(), made.look.body), ("Hero", 2));
        assert_eq!(a.characters("ann", "pw").unwrap().len(), 1);
        assert_eq!(
            a.login("bob", "pw", &new("hero"), 2, start),
            Err(AccountError::NameTaken)
        );
        a.login("ann", "pw", &new("Second"), 2, start).unwrap();
        assert_eq!(
            a.login("ann", "pw", &new("Third"), 2, start),
            Err(AccountError::TooManyCharacters)
        );
        let entered = a
            .login("ann", "pw", &Enter::Existing("second".into()), 2, start)
            .unwrap();
        assert_eq!(entered.name, "Second");
        let mut moved = entered;
        moved.level = 9;
        a.save("ann", moved).unwrap();
        let levels: Vec<u32> = a
            .characters("ann", "pw")
            .unwrap()
            .iter()
            .map(|c| c.level)
            .collect();
        assert_eq!(levels, [1, 9]);
        assert_eq!(a.characters("ann", "no"), Err(AccountError::WrongPassword));
        // The first login of an account without characters makes one named after it.
        assert_eq!(
            a.login("cid", "pw", &Enter::First, 1, start).unwrap().name,
            "cid"
        );
    }

    #[test]
    fn old_one_character_files_still_read() {
        let a = accounts("old");
        a.characters("dan", "pw").unwrap();
        let file = a.file("dan");
        let text = std::fs::read_to_string(&file).unwrap();
        let old = text.replace("characters: [],", "character: Some((name: \"dan\", cell: (1, 1), level: 4, exp: 0, money: 0, life: 0, mana: 0, bag: [], equipment: [])),");
        std::fs::write(&file, old).unwrap();
        let chars = a.characters("dan", "pw").unwrap();
        assert_eq!((chars.len(), chars[0].level), (1, 4));
    }
}
