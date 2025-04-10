use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV4};

use winnow::{
    ModalResult, Parser,
    ascii::{Caseless, digit1},
    combinator::{alt, opt},
    token::{literal, rest, take_till, take_until},
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// Remote identifier. This can be an IP:port pair or a bare IP.
pub enum Identifier {
    SocketAddr(SocketAddr),
    IpAddr(IpAddr),
}

fn match_for<'s>(s: &mut &'s str) -> ModalResult<&'s str> {
    Caseless("for=").parse_next(s)
}

fn match_port(s: &mut &str) -> ModalResult<Option<u16>> {
    Ok(opt((":", digit1))
        .parse_next(s)?
        .and_then(|(_, b)| b.parse::<u16>().ok()))
}

fn extract_ipv6(s: &mut &str) -> ModalResult<Identifier> {
    literal("\"[").parse_next(s)?;
    let ip = take_until(0.., ']').parse_to::<Ipv6Addr>().parse_next(s)?;
    literal("]").parse_next(s)?;
    let port = match_port.parse_next(s)?;

    match (ip, port) {
        (ip, None) => Ok(Identifier::IpAddr(IpAddr::V6(ip))),
        (ip, Some(port)) => Ok(Identifier::SocketAddr(SocketAddr::new(
            IpAddr::V6(ip),
            port,
        ))),
    }
}

fn extract_ipv4(s: &mut &str) -> ModalResult<Identifier> {
    let ip = take_till(0.., ':').parse_to().parse_next(s)?;
    let port = match_port.parse_next(s)?;

    match (ip, port) {
        (ip, None) => Ok(Identifier::IpAddr(IpAddr::V4(ip))),
        (ip, Some(port)) => Ok(Identifier::SocketAddr(SocketAddr::V4(SocketAddrV4::new(
            ip, port,
        )))),
    }
}

fn extract_identifier(s: &mut &str) -> ModalResult<Identifier> {
    alt((extract_ipv6, extract_ipv4)).parse_next(s)
}

pub fn extract_for(s: &mut &str) -> ModalResult<Identifier> {
    match_for.parse_next(s)?;

    let id = extract_identifier.parse_next(s)?;

    rest.parse_next(s)?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    #[test]
    fn extracts_for() -> color_eyre::Result<()> {
        assert_eq!(
            extract_for
                .parse("for=1.2.3.4")
                .map_err(|a| color_eyre::eyre::eyre!("Identifier parsing error:\n{a}"))?,
            Identifier::IpAddr(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)))
        );

        assert_eq!(
            extract_for
                .parse("fOr=1.2.3.4:1234")
                .map_err(|a| color_eyre::eyre::eyre!("Identifier parsing error:\n{a}"))?,
            Identifier::SocketAddr(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234))
        );

        assert_eq!(
            extract_for
                .parse("FoR=\"[::1]:1234\"")
                .map_err(|a| color_eyre::eyre::eyre!("Identifier parsing error:\n{a}"))?,
            Identifier::SocketAddr(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)),
                1234
            ))
        );

        assert_eq!(
            extract_for
                .parse("FOR=\"[::1]\"")
                .map_err(|a| color_eyre::eyre::eyre!("Identifier parsing error:\n{a}"))?,
            Identifier::IpAddr(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
        );

        Ok(())
    }
}
