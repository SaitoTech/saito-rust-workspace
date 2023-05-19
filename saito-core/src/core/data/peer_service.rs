use std::io::{Error, ErrorKind};

use log::warn;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerService {
    pub service: String,
    pub domain: String,
    pub name: String,
}

impl From<String> for PeerService {
    fn from(value: String) -> Self {
        let values: Vec<&str> = value.split("|").collect();
        PeerService {
            service: values[0].to_string(),
            domain: values[1].to_string(),
            name: values[2].to_string(),
        }
    }
}

impl Into<String> for PeerService {
    fn into(self) -> String {
        self.service + "|" + self.domain.as_str() + "|" + self.name.as_str()
    }
}

impl PeerService {
    pub fn serialize_services(services: &Vec<PeerService>) -> Vec<u8> {
        let str: String = services
            .iter()
            .map(|service| {
                let str: String = service.clone().into();
                str
            })
            .collect::<Vec<String>>()
            .join(";");
        str.as_bytes().to_vec()
    }
    pub fn deserialize_services(buffer: Vec<u8>) -> Result<Vec<PeerService>, Error> {
        let str = String::from_utf8(buffer);
        if str.is_err() {
            warn!("failed parsing services. {:?}", str.err().unwrap());
            return Err(Error::from(ErrorKind::InvalidData));
        }
        let str = str.unwrap();
        let strings = str.split(";");
        let services: Vec<PeerService> = strings.map(|str| str.to_string().into()).collect();
        Ok(services)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::data::peer_service::PeerService;

    #[test]
    fn test_serialize() {
        let mut services = vec![];
        services.push(PeerService {
            service: "service1".to_string(),
            domain: "domain1".to_string(),
            name: "name1".to_string(),
        });
        services.push(PeerService {
            service: "service2".to_string(),
            domain: "".to_string(),
            name: "name2".to_string(),
        });
        services.push(PeerService {
            service: "service3".to_string(),
            domain: "domain3".to_string(),
            name: "".to_string(),
        });
        services.push(PeerService {
            service: "service4".to_string(),
            domain: "".to_string(),
            name: "".to_string(),
        });

        let buffer = PeerService::serialize_services(&services);

        assert!(buffer.len() > 0);

        let result = PeerService::deserialize_services(buffer);
        assert!(result.is_ok());
        let services = result.unwrap();
        assert_eq!(services.len(), 4);

        let service = services.get(0).unwrap();
        assert_eq!(service.service, "service1");
        assert_eq!(service.domain, "domain1");
        assert_eq!(service.name, "name1");

        let service = services.get(1).unwrap();
        assert_eq!(service.service, "service2");
        assert_eq!(service.domain, "");
        assert_eq!(service.name, "name2");

        let service = services.get(2).unwrap();
        assert_eq!(service.service, "service3");
        assert_eq!(service.domain, "domain3");
        assert_eq!(service.name, "");

        let service = services.get(3).unwrap();
        assert_eq!(service.service, "service4");
        assert_eq!(service.domain, "");
        assert_eq!(service.name, "");
    }
}
