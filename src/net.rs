use core::{net::SocketAddr, str::FromStr};

use socket2::{Domain, Socket, Type};

pub fn get_tcp_socket(addr: &str) -> color_eyre::Result<tokio::net::TcpListener> {
    let addr = SocketAddr::from_str(addr)?;

    let socket = match addr {
        SocketAddr::V4(_) => Socket::new(Domain::IPV4, Type::STREAM, None)?,
        SocketAddr::V6(_) => Socket::new(Domain::IPV6, Type::STREAM, None)?,
    };

    socket.set_reuse_port(true)?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.set_tcp_nodelay(true)?;
    socket.bind(&addr.into())?;
    socket.listen(4096)?;

    let listener = std::net::TcpListener::from(socket);

    let listener = tokio::net::TcpListener::from_std(listener)?;

    Ok(listener)
}
