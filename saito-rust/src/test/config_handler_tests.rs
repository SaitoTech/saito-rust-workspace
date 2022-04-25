#[cfg(test)]

mod test {
    use std::io::ErrorKind;
    use saito_core::core::data::configuration::Configuration;
    use crate::ConfigHandler;

    #[test]
    fn load_config_from_existing_file() {
        let path = String::from("src/test/test_data/config_handler_tests.json");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_ok());
        let configs = result.unwrap();
        assert_eq!(configs.server.host, String::from("localhost"));
        assert_eq!(configs.server.port, 12101);
        assert_eq!(configs.server.protocol, String::from("http"));
        assert_eq!(configs.server.endpoint.host, String::from("localhost"));
        assert_eq!(configs.server.endpoint.port, 12101);
        assert_eq!(configs.server.endpoint.protocol, String::from("http"));
    }

    #[test]
    fn load_config_from_bad_file_format() {
        let path = String::from("src/test/test_data/config_handler_tests_bad_format.xml");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn load_config_from_non_existing_file() {
        let path = String::from("badfilename.json");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().kind(), ErrorKind::InvalidInput);
    }
}