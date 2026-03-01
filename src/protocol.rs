use libp2p::StreamProtocol;

pub const DEFAULT_NAMESPACE: &str = "sape";
pub const TUNNEL_PROTOCOL_VERSION: &str = "1.0.0";
pub const JUMP_PROTOCOL_VERSION: &str = "1.0.0";
pub const IDENTIFY_VERSION: &str = "0.1.0";

pub fn tunnel_protocol(namespace: &str) -> StreamProtocol {
    StreamProtocol::try_from_owned(format!(
        "/{namespace}/tunnel/{TUNNEL_PROTOCOL_VERSION}"
    ))
    .expect("valid protocol string")
}

pub fn jump_protocol(namespace: &str) -> StreamProtocol {
    StreamProtocol::try_from_owned(format!(
        "/{namespace}/jump/{JUMP_PROTOCOL_VERSION}"
    ))
    .expect("valid protocol string")
}

pub fn relay_identify_protocol(namespace: &str) -> String {
    format!("/{namespace}-relay/{IDENTIFY_VERSION}")
}

pub fn client_identify_protocol(namespace: &str) -> String {
    format!("/{namespace}-client/{IDENTIFY_VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunnel_protocol_string() {
        assert_eq!(
            tunnel_protocol("sape").to_string(),
            "/sape/tunnel/1.0.0"
        );
    }

    #[test]
    fn jump_protocol_string() {
        assert_eq!(
            jump_protocol("sape").to_string(),
            "/sape/jump/1.0.0"
        );
    }

    #[test]
    fn relay_identify_string() {
        assert_eq!(
            relay_identify_protocol("sape"),
            "/sape-relay/0.1.0"
        );
    }

    #[test]
    fn client_identify_string() {
        assert_eq!(
            client_identify_protocol("sape"),
            "/sape-client/0.1.0"
        );
    }

    #[test]
    fn custom_namespace() {
        assert_eq!(
            tunnel_protocol("mynet").to_string(),
            "/mynet/tunnel/1.0.0"
        );
    }
}
