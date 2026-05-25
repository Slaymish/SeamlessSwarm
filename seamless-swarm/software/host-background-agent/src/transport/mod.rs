use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use socket2::{Socket, Domain, Type, Protocol};
use nng::{Socket as NngSocket, Protocol as NngProtocol};

pub struct MdnsResponder {
    service_name: String,
    port: u16,
    socket: UdpSocket,
}

impl MdnsResponder {
    pub fn new(service_name: &str, port: u16) -> Result<Self, String> {
        let ip = Ipv4Addr::new(224, 0, 0, 251);
        let bind_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 5353);

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| e.to_string())?;

        socket.set_reuse_address(true).map_err(|e| e.to_string())?;

        let actual_socket = match socket.bind(&bind_addr.into()) {
            Ok(_) => {
                socket.join_multicast_v4(&ip, &Ipv4Addr::UNSPECIFIED).map_err(|e| e.to_string())?;
                socket.into()
            }
            Err(_) => {
                let ephemeral_bind = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0);
                let fallback = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
                    .map_err(|e| e.to_string())?;
                fallback.set_reuse_address(true).map_err(|e| e.to_string())?;
                fallback.bind(&ephemeral_bind.into()).map_err(|e| e.to_string())?;
                fallback.join_multicast_v4(&ip, &Ipv4Addr::UNSPECIFIED).map_err(|e| e.to_string())?;
                fallback.into()
            }
        };

        Ok(Self {
            service_name: service_name.to_string(),
            port,
            socket: actual_socket,
        })
    }

    pub async fn start_broadcast(&self) -> Result<(), String> {
        let dest = SocketAddr::new(Ipv4Addr::new(224, 0, 0, 251).into(), 5353);
        let payload = format!("SEAMLESS-SWARM:REGISTER:{}:{}", self.service_name, self.port);
        let bytes = payload.into_bytes();
        let socket_clone = self.socket.try_clone().map_err(|e| e.to_string())?;

        tokio::spawn(async move {
            loop {
                let _ = socket_clone.send_to(&bytes, &dest);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });

        Ok(())
    }
}

pub struct NngClient {
    endpoint: String,
    socket: Option<NngSocket>,
}

impl NngClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            socket: None,
        }
    }

    pub async fn connect(&mut self) -> Result<(), String> {
        let socket = NngSocket::new(NngProtocol::Push0).map_err(|e| e.to_string())?;
        socket.dial(&self.endpoint).map_err(|e| e.to_string())?;
        self.socket = Some(socket);
        Ok(())
    }

    pub async fn send_payload(&self, payload: &[u8]) -> Result<(), String> {
        if let Some(ref socket) = self.socket {
            socket.send(payload).map_err(|(_, err)| err.to_string())?;
            Ok(())
        } else {
            Err("Not connected".to_string())
        }
    }
}
