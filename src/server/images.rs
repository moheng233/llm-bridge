//! image_url 只允许内联图片或固定解析后的公网目标。
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use crate::types::LanguageModelDataPart;
use base64::Engine;
use futures_util::StreamExt;

pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..128).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..32).contains(&b))
        || (a == 192 && (b == 168 || (b == 0 && (c == 0 || c == 2)) || (b == 88 && c == 99)))
        || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113))
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => {
            if let Some(ip) = ip.to_ipv4_mapped() {
                return public_v4(ip);
            }
            let parts = ip.segments();
            (parts[0] & 0xe000) == 0x2000
                && !(parts[0] == 0x2001 && (parts[1] < 0x0200 || parts[1] == 0x0db8))
                && parts[0] != 0x2002
                && !(parts[0] == 0x3fff && parts[1] < 0x1000)
        }
    }
}

pub async fn resolve_image_url(raw: &str) -> Result<LanguageModelDataPart, String> {
    if let Some(inline) = raw.strip_prefix("data:") {
        let (meta, encoded) = inline.split_once(',').ok_or("invalid image data URI")?;
        let mime = meta
            .strip_suffix(";base64")
            .ok_or("image data URI requires base64 encoding")?;
        if !mime.starts_with("image/") {
            return Err("image data URI requires an image MIME type".into());
        }
        let encoded = encoded.trim();
        if encoded.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 {
            return Err("image exceeds 10 MiB limit".into());
        }
        let data = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| format!("invalid image base64: {e}"))?;
        if data.len() > MAX_IMAGE_BYTES {
            return Err("image exceeds 10 MiB limit".into());
        }
        return Ok(LanguageModelDataPart {
            mime_type: mime.into(),
            data,
        });
    }
    let url = reqwest::Url::parse(raw).map_err(|e| format!("invalid image URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("image URL must use data, http or https".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("image URL credentials are not allowed".into());
    }
    let host = url
        .host_str()
        .ok_or("image URL has no host")?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let port = url.port_or_known_default().ok_or("image URL has no port")?;
    let addresses: Vec<std::net::SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![(ip, port).into()]
    } else {
        tokio::time::timeout(
            Duration::from_secs(3),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| "image DNS lookup timed out")?
        .map_err(|e| format!("image DNS lookup failed: {e}"))?
        .collect()
    };
    if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
        return Err("image URL must resolve exclusively to public addresses".into());
    }
    let client = crate::http::client_builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &addresses)
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("image HTTP client failed: {e}"))?;
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|e| format!("image fetch failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "image fetch returned HTTP {}; redirects are not followed",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_IMAGE_BYTES as u64)
    {
        return Err("image exceeds 10 MiB limit".into());
    }
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/png")
        .split(';')
        .next()
        .unwrap_or("image/png")
        .trim()
        .to_string();
    if !mime_type.starts_with("image/") {
        return Err("image response has a non-image MIME type".into());
    }
    let mut data = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or(0)
            .min(MAX_IMAGE_BYTES as u64) as usize,
    );
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| format!("image body read failed: {e}"))?;
        if chunk.len() > MAX_IMAGE_BYTES - data.len() {
            return Err("image exceeds 10 MiB limit".into());
        }
        data.extend_from_slice(&chunk);
    }
    Ok(LanguageModelDataPart { mime_type, data })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn blocks_internal_and_mapped_targets_before_fetch() {
        for target in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "[::1]",
            "[::ffff:127.0.0.1]",
            "[fc00::1]",
            "[64:ff9b::a00:1]",
        ] {
            let error = resolve_image_url(&format!("http://{target}/image.png"))
                .await
                .unwrap_err();
            assert!(error.contains("public addresses"), "{target}: {error}");
        }
    }
    #[test]
    fn permits_public_unicast_but_not_documentation_or_transition_ranges() {
        assert!(public_ip("8.8.8.8".parse().unwrap()));
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
        for ip in [
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "2001:db8::1",
            "2002:7f00:1::",
            "3fff::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
    }
}
