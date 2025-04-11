use std::net::IpAddr;

use winnow::{
    ModalResult, Parser,
    ascii::Caseless,
    combinator::alt,
    stream::Stream,
    token::{literal, take_till, take_until},
};

/// Check if a header value string begins with `for=`, in order to
/// determine whether it is a valid value for a Forwarded header, and
/// is a part that we are interested in.
fn match_for<'s>(prefix: &mut &'s str) -> ModalResult<&'s str> {
    Caseless("for=").parse_next(prefix)
}

/// Extracts the IPv6 address from the identifier string. Expects the format
/// `"[::1]:8080"`. IPv6 addresses in the Forwarded header are always wrapped in `"`,
/// with the address component wrapped in square brackets. The port component `:8080`
/// may or may not be present in the identifier. For the purposes of `nailpit`, we discard
/// the port info.
fn extract_ipv6(identifier: &mut &str) -> ModalResult<IpAddr> {
    literal("\"[").parse_next(identifier)?;
    take_until(0.., ']')
        .parse_to()
        .map(IpAddr::V6)
        .parse_next(identifier)
}

/// Extracts the IPv4 address from the identifier string. Expects the format
/// `192.168.0.1:8080`. IPv4 addresses are not wrapped with `"` in contrast to IPv6
/// addresses. The port component `:8080` may or may not be present in the identifier.
/// For the purposes of `nailpit`, we discard the port info.
fn extract_ipv4(identifier: &mut &str) -> ModalResult<IpAddr> {
    take_till(0.., ':')
        .parse_to()
        .map(IpAddr::V4)
        .parse_next(identifier)
}

/// Extracts the identifier from the Forwarded for value. Attempts to parse the identifier part
/// as either an IPv6 address or IPv4 address. The Forwarded for format supports more identifier
/// types, but we discard those as they are useless to us.
fn extract_identifier(identifier: &mut &str) -> ModalResult<IpAddr> {
    alt((extract_ipv6, extract_ipv4)).parse_next(identifier)
}

/// Extracts the identifier from the Forwarded for value. First it checks for the correct
/// prefix `for=`, then attempts to extract the identifier.
pub fn extract_for(header_part: &mut &str) -> ModalResult<IpAddr> {
    match_for.parse_next(header_part)?;

    let id = extract_identifier.parse_next(header_part)?;

    header_part.finish();

    Ok(id)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    #[test]
    fn extracts_for_header_value() -> color_eyre::Result<()> {
        assert_eq!(
            extract_for
                .parse("for=1.2.3.4")
                .map_err(|a| color_eyre::eyre::eyre!("Identifier parsing error:\n{a}"))?,
            IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))
        );

        assert_eq!(
            extract_for
                .parse("fOr=1.2.3.4:1234")
                .map_err(|a| color_eyre::eyre::eyre!("Identifier parsing error:\n{a}"))?,
            IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))
        );

        assert_eq!(
            extract_for
                .parse("FoR=\"[::1]:1234\"")
                .map_err(|a| color_eyre::eyre::eyre!("Identifier parsing error:\n{a}"))?,
            IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1))
        );

        assert_eq!(
            extract_for
                .parse("FOR=\"[::1]\"")
                .map_err(|a| color_eyre::eyre::eyre!("Identifier parsing error:\n{a}"))?,
            IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1))
        );

        Ok(())
    }
}
