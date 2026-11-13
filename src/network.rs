//! Best-effort adapter telemetry. No external requests or RustDesk commands.
use serde::{Deserialize, Deserializer};
use std::net::IpAddr;

pub fn deserialize_ips<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<IpAddr>>, D::Error> {
    let ips = Vec::<IpAddr>::deserialize(d)?;
    if ips.len() > 64 || ips.iter().enumerate().any(|(i, ip)| ips[..i].contains(ip)) {
        return Err(serde::de::Error::custom("invalid local IP list"));
    }
    Ok(Some(ips))
}

fn normalize(mut ips: Vec<IpAddr>) -> Vec<IpAddr> {
    ips.retain(|ip| {
        !ip.is_loopback()
            && !ip.is_unspecified()
            && !ip.is_multicast()
            && !matches!(ip, IpAddr::V6(v6) if v6.is_unicast_link_local())
    });
    ips.sort_unstable();
    ips.dedup();
    ips.truncate(64);
    ips
}

#[cfg(not(windows))]
pub(crate) fn collect() -> Option<Vec<IpAddr>> {
    None
}

#[cfg(windows)]
pub(crate) fn collect() -> Option<Vec<IpAddr>> {
    use windows_sys::Win32::{
        Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, NO_ERROR},
        NetworkManagement::{IpHelper::*, Ndis::IfOperStatusUp},
        Networking::WinSock::*,
    };
    let mut size = 15_000u32;
    for _ in 0..3 {
        if size > 1024 * 1024 {
            return None;
        }
        // u64 storage provides the alignment required by IP_ADAPTER_ADDRESSES.
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let head = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        // SAFETY: owned, aligned writable buffer; all returned pointers are used
        // only after success and before the backing allocation is dropped.
        let result = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC as u32,
                GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
                std::ptr::null(),
                head,
                &mut size,
            )
        };
        if result == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        if result == ERROR_NO_DATA {
            return Some(Vec::new());
        }
        if result != NO_ERROR {
            return None;
        }
        let mut ips = Vec::new();
        unsafe {
            let mut adapter = head;
            while !adapter.is_null() {
                if (*adapter).OperStatus == IfOperStatusUp {
                    let mut address = (*adapter).FirstUnicastAddress;
                    while !address.is_null() {
                        let socket = (*address).Address;
                        if (*address).DadState == IpDadStatePreferred
                            && !socket.lpSockaddr.is_null()
                        {
                            let family = (*socket.lpSockaddr).sa_family;
                            if family == AF_INET
                                && socket.iSockaddrLength as usize >= size_of::<SOCKADDR_IN>()
                            {
                                let value = &*socket.lpSockaddr.cast::<SOCKADDR_IN>();
                                ips.push(IpAddr::V4(std::net::Ipv4Addr::from(
                                    value.sin_addr.S_un.S_addr.to_ne_bytes(),
                                )));
                            } else if family == AF_INET6
                                && socket.iSockaddrLength as usize >= size_of::<SOCKADDR_IN6>()
                            {
                                let value = &*socket.lpSockaddr.cast::<SOCKADDR_IN6>();
                                ips.push(IpAddr::V6(std::net::Ipv6Addr::from(
                                    value.sin6_addr.u.Byte,
                                )));
                            }
                        }
                        address = (*address).Next;
                    }
                }
                adapter = (*adapter).Next;
            }
        }
        return Some(normalize(ips));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn addresses_are_filtered_sorted_and_deduplicated() {
        let ips = [
            "192.168.1.2",
            "::1",
            "0.0.0.0",
            "127.0.0.1",
            "224.0.0.1",
            "fe80::1",
            "2001:db8::1",
            "192.168.1.2",
        ];
        assert_eq!(
            normalize(ips.map(|s| s.parse().unwrap()).to_vec()),
            [
                "192.168.1.2".parse::<IpAddr>().unwrap(),
                "2001:db8::1".parse().unwrap()
            ]
        );
    }
    #[cfg(windows)]
    #[test]
    fn windows_adapter_enumeration_runs_without_rustdesk() {
        let ips = collect().expect("Windows adapter enumeration failed");
        assert_eq!(normalize(ips.clone()), ips);
    }
}
