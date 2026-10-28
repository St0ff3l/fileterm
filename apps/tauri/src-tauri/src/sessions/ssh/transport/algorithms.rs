/// 构造兼容老服务器的算法偏好列表。
///
/// russh 0.62 的 `Preferred::DEFAULT` 注释明确"SHA-1 MAC variants are
/// excluded from defaults"，KEX 也只列出 SHA-2 系（DH_G14_SHA256 等）。
/// 这对 OpenSSH 4.x/5.x 时代的老服务器（只支持 hmac-sha1 / diffie-hellman
/// -group14-sha1 / diffie-hellman-group1-sha1）会导致 `NoCommonAlgo` 握手
/// 失败。
///
/// 这里把 SHA-1 类算法**追加到默认列表末尾**——SHA-2 仍然优先，只有当
/// 服务器不支持 SHA-2 时才回退到 SHA-1。RSA-SHA1 host key 已在默认列表
/// （`Algorithm::Rsa { hash: None }` 即 ssh-rsa），无需额外追加。
fn build_legacy_preferred() -> russh::Preferred {
    use std::borrow::Cow;

    let mut kex_list: Vec<russh::kex::Name> = russh::Preferred::DEFAULT.kex.to_vec();
    // SHA-1 KEX（按强度降序：group14 > group1 > gex-sha1）
    kex_list.push(russh::kex::DH_G14_SHA1);
    kex_list.push(russh::kex::DH_G1_SHA1);
    kex_list.push(russh::kex::DH_GEX_SHA1);

    let mut mac_list: Vec<russh::mac::Name> = russh::Preferred::DEFAULT.mac.to_vec();
    // SHA-1 MAC（ETM 优先于非 ETM，与默认列表风格一致）
    mac_list.push(russh::mac::HMAC_SHA1_ETM);
    mac_list.push(russh::mac::HMAC_SHA1);

    russh::Preferred {
        kex: Cow::Owned(kex_list),
        host_key_certificates: russh::Preferred::DEFAULT.host_key_certificates.clone(),
        key: russh::Preferred::DEFAULT.key.clone(),
        cipher: russh::Preferred::DEFAULT.cipher.clone(),
        mac: Cow::Owned(mac_list),
        compression: russh::Preferred::DEFAULT.compression.clone(),
    }
}

// Keep algorithm policy in one place for direct and jump-host SSH/SFTP sessions.
fn build_ssh_algorithm_config(
    legacy_algorithms: bool,
) -> Result<russh::client::Config, russh::Error> {
    Ok(russh::client::Config {
        // Some legacy servers return a fixed 1024-bit group even for GEX-SHA256.
        // Permit that only with explicit opt-in; still request a preferred 8192-bit
        // group. The vendor's Comware SHA-1 override keeps its own preferred 1024.
        gex: if legacy_algorithms {
            russh::client::GexParams::for_legacy_client_config(1024, 8192, 8192)?
        } else {
            russh::client::GexParams::default()
        },
        comware_legacy_gex: legacy_algorithms,
        preferred: if legacy_algorithms {
            build_legacy_preferred()
        } else {
            russh::Preferred::default()
        },
        ..Default::default()
    })
}

#[cfg(test)]
mod legacy_gex_tests {
    include!("algorithms_tests.rs");
}
