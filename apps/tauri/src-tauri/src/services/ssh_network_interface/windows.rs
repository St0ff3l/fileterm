use super::{AdapterInfo, Preference};
use std::{ffi::CStr, io, net::SocketAddr, os::windows::io::AsRawSocket, ptr};
use tokio::net::{TcpSocket, TcpStream};
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::ERROR_BUFFER_OVERFLOW,
    NetworkManagement::{
        IpHelper::{GetAdaptersAddresses, IF_TYPE_SOFTWARE_LOOPBACK, IP_ADAPTER_ADDRESSES_LH},
        Ndis::IfOperStatusUp,
    },
    Networking::WinSock::{
        setsockopt, WSAGetLastError, AF_INET, AF_INET6, AF_UNSPEC, IPPROTO_IP, IPPROTO_IPV6,
        IPV6_UNICAST_IF, IP_UNICAST_IF,
    },
};

struct Adapter {
    info: AdapterInfo,
    ipv4_index: u32,
    ipv6_index: u32,
}

fn adapters() -> Result<Vec<Adapter>, String> {
    let mut bytes = 15_000u32;
    for _ in 0..3 {
        // The API writes a linked list of aligned structures into this buffer.
        let mut buffer = vec![0u64; (bytes as usize).div_ceil(8)];
        let mut current = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let result =
            unsafe { GetAdaptersAddresses(AF_UNSPEC.into(), 0, ptr::null(), current, &mut bytes) };
        if result == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        if result != 0 {
            return Err(format!(
                "SSH outbound interface enumeration failed: {}",
                io::Error::from_raw_os_error(result as i32)
            ));
        }
        let mut adapters = Vec::new();
        while let Some(entry) = unsafe { current.as_ref() } {
            current = entry.Next;
            if entry.IfType == IF_TYPE_SOFTWARE_LOOPBACK || entry.AdapterName.is_null() {
                continue;
            }
            let Ok(id) = unsafe { CStr::from_ptr(entry.AdapterName.cast()) }
                .to_str()
                .unwrap_or_default()
                .parse::<Uuid>()
            else {
                continue;
            };
            let name = if entry.FriendlyName.is_null() {
                id.to_string()
            } else {
                let length = (0..)
                    .find(|offset| unsafe { *entry.FriendlyName.add(*offset) == 0 })
                    .unwrap();
                String::from_utf16_lossy(unsafe {
                    std::slice::from_raw_parts(entry.FriendlyName, length)
                })
            };
            let (ipv4_index, ipv6_index) = (
                unsafe { entry.Anonymous1.Anonymous.IfIndex },
                entry.Ipv6IfIndex,
            );
            let (mut ipv4, mut ipv6) = (false, false);
            let mut unicast = entry.FirstUnicastAddress;
            while let Some(address) = unsafe { unicast.as_ref() } {
                if let Some(socket) = unsafe { address.Address.lpSockaddr.as_ref() } {
                    ipv4 |= ipv4_index != 0 && socket.sa_family == AF_INET;
                    ipv6 |= ipv6_index != 0 && socket.sa_family == AF_INET6;
                }
                unicast = address.Next;
            }
            adapters.push(Adapter {
                info: AdapterInfo {
                    id,
                    name,
                    online: entry.OperStatus == IfOperStatusUp,
                    ipv4,
                    ipv6,
                },
                ipv4_index,
                ipv6_index,
            });
        }
        return Ok(adapters);
    }
    Err("SSH outbound interface list changed during enumeration".into())
}

pub(super) fn list() -> Result<Vec<AdapterInfo>, String> {
    Ok(adapters()?
        .into_iter()
        .map(|adapter| adapter.info)
        .collect())
}

fn interface_index(
    adapters: &[Adapter],
    preference: &Preference,
    ipv4: bool,
) -> Result<(u32, String), String> {
    let adapter = adapters.iter().find(|adapter| adapter.info.id == preference.adapter_id).ok_or_else(|| {
        let name = preference.display_name.clone().filter(|name| !name.is_empty())
            .unwrap_or_else(|| preference.adapter_id.to_string());
        format!("SSH outbound interface {name} is missing. Select an available interface or System default.")
    })?;
    let name = &adapter.info.name;
    if !adapter.info.online {
        return Err(format!("SSH outbound interface {name} is offline"));
    }
    let index = match ipv4 {
        true if adapter.info.ipv4 => adapter.ipv4_index,
        false if adapter.info.ipv6 => adapter.ipv6_index,
        _ => {
            return Err(format!(
                "SSH outbound interface {name} has no {} address",
                if ipv4 { "IPv4" } else { "IPv6" }
            ))
        }
    };
    Ok((index, name.clone()))
}

pub(super) async fn connect(
    preference: Preference,
    address: SocketAddr,
) -> Result<TcpStream, String> {
    let ipv4 = address.is_ipv4();
    let (index, name) = interface_index(&adapters()?, &preference, ipv4)?;
    let (socket, level, option, value) = if ipv4 {
        (
            TcpSocket::new_v4(),
            IPPROTO_IP,
            IP_UNICAST_IF,
            index.to_be(),
        )
    } else {
        (TcpSocket::new_v6(), IPPROTO_IPV6, IPV6_UNICAST_IF, index)
    };
    let socket = socket.map_err(|error| {
        format!("SSH outbound interface {name}: socket creation failed: {error}")
    })?;
    // Winsock expects network byte order for IPv4 and native byte order for IPv6.
    let result = unsafe {
        setsockopt(
            socket.as_raw_socket() as usize,
            level,
            option,
            (&value as *const u32).cast(),
            4,
        )
    };
    if result != 0 {
        let error = io::Error::from_raw_os_error(unsafe { WSAGetLastError() });
        return Err(format!(
            "SSH outbound interface {name}: bind failed: {error}"
        ));
    }
    socket.connect(address).await.map_err(|error| {
        format!("SSH outbound interface {name}: connect to {address} failed: {error}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_current_family_index_and_rejects_missing_or_offline_adapter() {
        let id = Uuid::from_u128(1);
        let preference = Preference {
            adapter_id: id,
            display_name: Some("Test LAN".into()),
        };
        let mut adapters = [Adapter {
            info: AdapterInfo {
                id,
                name: "Test LAN".into(),
                online: true,
                ipv4: true,
                ipv6: false,
            },
            ipv4_index: 7,
            ipv6_index: 8,
        }];
        assert_eq!(interface_index(&adapters, &preference, true).unwrap().0, 7);
        adapters[0].ipv4_index = 19;
        assert_eq!(interface_index(&adapters, &preference, true).unwrap().0, 19);
        assert!(interface_index(&adapters, &preference, false)
            .unwrap_err()
            .contains("IPv6"));
        adapters[0].info.online = false;
        assert!(interface_index(&adapters, &preference, true)
            .unwrap_err()
            .contains("offline"));
        assert!(interface_index(&[], &preference, true)
            .unwrap_err()
            .contains("missing"));
    }
}
