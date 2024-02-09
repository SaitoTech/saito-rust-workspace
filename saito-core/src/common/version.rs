use crate::core::util::serialize::Serialize;
use log::warn;
use std::cmp::Ordering;
use std::io::{Error, ErrorKind};

const VERSION_SIZE: u8 = 4;

#[derive(Debug, Default, Clone)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u16,
}

impl Serialize<Self> for Version {
    fn serialize(&self) -> Vec<u8> {
        let mut v = [self.major, self.minor].to_vec();
        v.append(&mut self.patch.to_be_bytes().to_vec());
        v
    }

    fn deserialize(buffer: &Vec<u8>) -> Result<Self, Error> {
        if buffer.len() < VERSION_SIZE as usize {
            warn!(
                "buffer size : {:?} cannot be parsed as a version",
                buffer.len()
            );
            return Err(Error::from(ErrorKind::InvalidData));
        }
        // TODO : refactor and optimize this
        let ver = Version {
            major: *buffer.get(0).unwrap(),
            minor: *buffer.get(1).unwrap(),
            patch: u16::from_be_bytes([*buffer.get(2).unwrap(), *buffer.get(3).unwrap()]),
        };

        Ok(ver)
    }
}

impl Eq for Version {}

impl PartialEq<Self> for Version {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major && self.minor == other.minor && self.patch == other.patch
    }
}

impl PartialOrd<Self> for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.major < other.major {
            return Some(Ordering::Less);
        } else if self.major > other.major {
            return Some(Ordering::Greater);
        }
        if self.minor < other.minor {
            return Some(Ordering::Less);
        } else if self.minor > other.minor {
            return Some(Ordering::Greater);
        }

        self.patch.partial_cmp(&other.patch)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::common::version::{Version, VERSION_SIZE};
    use crate::core::util::serialize::Serialize;

    #[test]
    fn test_version_serialization() {
        let version = Version {
            major: 2,
            minor: 45,
            patch: 1243,
        };
        let buffer = version.serialize();
        assert_eq!(buffer.len(), VERSION_SIZE as usize);
        let version2 = Version::deserialize(&buffer)
            .expect("can't deserialize given buffer to version object");

        assert_eq!(version, version2);
    }
    #[test]
    fn test_version_compare() {
        assert!(
            Version {
                major: 10,
                minor: 20,
                patch: 30,
            } < Version {
                major: 10,
                minor: 30,
                patch: 20,
            }
        );
        assert!(
            Version {
                major: 10,
                minor: 20,
                patch: 30,
            } > Version {
                major: 10,
                minor: 10,
                patch: 30,
            }
        );
        assert!(
            Version {
                major: 10,
                minor: 20,
                patch: 30,
            } < Version {
                major: 10,
                minor: 20,
                patch: 40,
            }
        );
    }
}
