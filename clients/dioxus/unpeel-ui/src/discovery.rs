//! Nearby-Host discovery (mDNS), ported from
//! `clients/native/UnpeelNative/Sources/UnpeelNative/NearbyHostBrowser.swift`.
//!
//! Bonjour is only a hint: choosing a row never grants access, and the
//! sealed one-time pairing code still authenticates the Host identity and
//! endpoint. The service type is `_unpeel-remote._tcp.local`; the Host
//! identity comes from the TXT record's `macid` key.
//!
//! mDNS itself is implemented here over `std::net::UdpSocket` (multicast
//! 224.0.0.251:5353) with a minimal DNS packet parser, so the Dioxus
//! launchers need no extra dependency for discovery.

use crate::i18n::t;

use std::collections::HashMap;
use std::net::{Ipv4Addr, UdpSocket};
use std::time::{Duration, Instant};

/// The mDNS service type Unpeel Hosts advertise.
pub const MDNS_SERVICE_TYPE: &str = "_unpeel-remote._tcp.local";
/// mDNS multicast group and port.
pub const MDNS_MULTICAST: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MDNS_PORT: u16 = 5353;

/// One discovered Host: its stable identity plus a display name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NearbyHostCandidate {
    pub host_id: String,
    pub name: String,
}

impl NearbyHostCandidate {
    pub fn id(&self) -> &str {
        &self.host_id
    }
}

/// Pure catalog logic (mirrors `NearbyHostCatalog`).
pub mod catalog {
    use super::NearbyHostCandidate;
    use std::collections::HashMap;

    /// Build a candidate from a service instance name and its TXT record.
    /// Returns `None` when the `macid` key is missing or blank.
    pub fn candidate(
        service_name: &str,
        txt: &HashMap<String, String>,
    ) -> Option<NearbyHostCandidate> {
        let host_id = txt
            .get("macid")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        let name = service_name.trim();
        Some(NearbyHostCandidate {
            host_id,
            name: if name.is_empty() {
                {
                    crate::i18n::t("discovery.unpeel_host")
                }
            } else {
                name.to_string()
            },
        })
    }

    /// Merge candidates: drop the excluded Host (this Controller), dedupe
    /// case-insensitively on host id, sort case-insensitively by name with
    /// host id as the tiebreak.
    pub fn merging(
        candidates: Vec<NearbyHostCandidate>,
        excluding_host_id: Option<&str>,
    ) -> Vec<NearbyHostCandidate> {
        let excluded = excluding_host_id
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        let mut by_id: HashMap<String, NearbyHostCandidate> = HashMap::new();
        for candidate in candidates {
            let key = candidate.host_id.to_lowercase();
            if excluded.as_deref() == Some(key.as_str()) {
                continue;
            }
            by_id.entry(key).or_insert(candidate);
        }
        let mut merged: Vec<NearbyHostCandidate> = by_id.into_values().collect();
        merged.sort_by(|a, b| {
            let an = a.name.to_lowercase();
            let bn = b.name.to_lowercase();
            an.cmp(&bn).then_with(|| a.host_id.cmp(&b.host_id))
        });
        merged
    }
}

/// Browser state (mirrors `NearbyHostBrowser.State`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscoveryState {
    Idle,
    Searching,
    Unavailable(String),
}

#[derive(Debug)]
pub enum DiscoveryError {
    Socket(String),
    Timeout,
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryError::Socket(e) => write!(f, "discovery socket: {e}"),
            DiscoveryError::Timeout => write!(f, "{}", { t("discovery.discovery_timed_out") }),
        }
    }
}

/// One blocking browse pass: send a PTR query for the Unpeel service type
/// and collect answers until `timeout` elapses. Returns raw candidates;
/// callers merge with [`catalog::merging`] (excluding their own Host id).
pub fn browse_once(timeout: Duration) -> Result<Vec<NearbyHostCandidate>, DiscoveryError> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| DiscoveryError::Socket(e.to_string()))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|e| DiscoveryError::Socket(e.to_string()))?;
    let query = build_ptr_query(MDNS_SERVICE_TYPE);
    socket
        .send_to(&query, (MDNS_MULTICAST, MDNS_PORT))
        .map_err(|e| DiscoveryError::Socket(e.to_string()))?;

    let deadline = Instant::now() + timeout;
    let mut instances: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut buf = [0u8; 4096];
    while Instant::now() < deadline {
        let n = match socket.recv(&mut buf) {
            Ok(n) => n,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue
            }
            Err(e) => return Err(DiscoveryError::Socket(e.to_string())),
        };
        for (instance, txt) in parse_mdns_response(&buf[..n]) {
            instances.entry(instance).or_default().extend(txt);
        }
    }
    if instances.is_empty() {
        return Err(DiscoveryError::Timeout);
    }
    Ok(instances
        .into_iter()
        .filter_map(|(instance, txt)| {
            let name = instance
                .strip_suffix("._unpeel-remote._tcp.local")
                .unwrap_or(&instance);
            catalog::candidate(name, &txt)
        })
        .collect())
}

fn build_ptr_query(service: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&[0x00, 0x00]); // id
    out.extend_from_slice(&[0x00, 0x00]); // flags: standard query
    out.extend_from_slice(&[0x00, 0x01]); // qdcount = 1
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // an/ns/ar = 0
    for label in service.split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out.extend_from_slice(&[0x00, 0x0c]); // QTYPE PTR
    out.extend_from_slice(&[0x00, 0x01]); // QCLASS IN
    out
}

/// Parse an mDNS response into (instance-name, txt-records) pairs.
/// Handles PTR (instance enumeration), TXT (key=value), SRV/A (ignored
/// here — identity comes from TXT `macid`, the address from pairing).
pub fn parse_mdns_response(packet: &[u8]) -> Vec<(String, HashMap<String, String>)> {
    let mut out = Vec::new();
    let Some(records) = parse_dns_records(packet) else {
        return out;
    };
    let mut ptr_targets: Vec<String> = Vec::new();
    let mut txt_by_name: HashMap<String, HashMap<String, String>> = HashMap::new();
    for record in records {
        match record {
            DnsRecord::Ptr { target, .. } => ptr_targets.push(target),
            DnsRecord::Txt { name, txt } => {
                txt_by_name.entry(name).or_default().extend(txt);
            }
            DnsRecord::Other => {}
        }
    }
    for target in ptr_targets {
        let txt = txt_by_name.remove(&target).unwrap_or_default();
        out.push((target, txt));
    }
    // TXT records without a preceding PTR (unsolicited) still count.
    for (name, txt) in txt_by_name {
        out.push((name, txt));
    }
    out
}

enum DnsRecord {
    Ptr {
        target: String,
    },
    Txt {
        name: String,
        txt: HashMap<String, String>,
    },
    Other,
}

struct DnsParser<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> DnsParser<'a> {
    fn u16(&mut self) -> Option<u16> {
        let b = self.buf.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }

    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let b = self.buf.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(b)
    }

    /// Read a possibly-compressed domain name.
    fn name(&mut self) -> Option<String> {
        let mut labels = Vec::new();
        let mut pos = self.pos;
        let mut jumped = false;
        let mut jumps = 0;
        loop {
            if jumps > 16 {
                return None;
            }
            let len = *self.buf.get(pos)?;
            if len & 0xC0 == 0xC0 {
                let b2 = *self.buf.get(pos + 1)?;
                let target = (((len & 0x3F) as usize) << 8) | b2 as usize;
                if !jumped {
                    self.pos = pos + 2;
                    jumped = true;
                }
                pos = target;
                jumps += 1;
                continue;
            }
            if len == 0 {
                if !jumped {
                    self.pos = pos + 1;
                }
                break;
            }
            let len = len as usize;
            let label = self.buf.get(pos + 1..pos + 1 + len)?;
            labels.push(String::from_utf8_lossy(label).into_owned());
            pos += 1 + len;
        }
        Some(labels.join("."))
    }
}

fn parse_dns_records(packet: &[u8]) -> Option<Vec<DnsRecord>> {
    let mut p = DnsParser {
        buf: packet,
        pos: 0,
    };
    p.bytes(2)?; // id
    let flags = p.u16()?;
    if flags & 0x8000 == 0 {
        return None; // not a response
    }
    let qd = p.u16()? as usize;
    let an = p.u16()? as usize;
    let ns = p.u16()? as usize;
    let ar = p.u16()? as usize;
    for _ in 0..qd {
        p.name()?;
        p.u16()?; // qtype
        p.u16()?; // qclass
    }
    let mut records = Vec::new();
    // mDNS responders put TXT (and SRV) records in the additional section as
    // often as in the answer section, so parse all three record sections.
    for _ in 0..an + ns + ar {
        let name = p.name()?;
        let rtype = p.u16()?;
        p.u16()?; // class
        p.u32_ttl()?;
        let rdlen = p.u16()? as usize;
        let rdata_end = p.pos + rdlen;
        match rtype {
            12 => {
                // PTR
                let target = p.name()?;
                records.push(DnsRecord::Ptr { target });
            }
            16 => {
                // TXT
                let mut txt = HashMap::new();
                while p.pos < rdata_end {
                    let len = *p.buf.get(p.pos)? as usize;
                    p.pos += 1;
                    let s = p.bytes(len)?;
                    let s = String::from_utf8_lossy(s);
                    if let Some((k, v)) = s.split_once('=') {
                        txt.insert(k.to_string(), v.to_string());
                    } else {
                        txt.insert(s.into_owned(), String::new());
                    }
                }
                records.push(DnsRecord::Txt { name, txt });
            }
            _ => {
                p.pos = rdata_end;
                records.push(DnsRecord::Other);
            }
        }
    }
    Some(records)
}

impl<'a> DnsParser<'a> {
    fn u32_ttl(&mut self) -> Option<u32> {
        let b = self.buf.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Dioxus component: the nearby-Host picker sheet.
pub mod component {
    use super::*;
    use dioxus::prelude::*;

    #[component]
    pub fn DiscoverySheet(
        candidates: Vec<NearbyHostCandidate>,
        state: DiscoveryState,
        on_select: EventHandler<NearbyHostCandidate>,
        on_close: EventHandler<()>,
    ) -> Element {
        rsx! {
            div { class: "discovery-sheet",
                div { class: "discovery-header",
                    h2 { {t("discovery.nearby_hosts")} }
                    button { onclick: move |_| on_close.call(()), {t("discovery.close")} }
                }
                match &state {
                    DiscoveryState::Searching => rsx! { p { class: "discovery-status", {t("discovery.searching_the_local_network")} } },
                    DiscoveryState::Unavailable(e) => rsx! { p { class: "discovery-error", "Discovery unavailable: {e}" } },
                    DiscoveryState::Idle => rsx! {},
                }
                if candidates.is_empty() {
                    p { class: "discovery-empty", "No Unpeel Hosts found nearby. They appear here when the Host app advertises itself on this network." }
                } else {
                    ul { class: "discovery-list",
                        for c in candidates {
                            li {
                                key: "{c.host_id}",
                                button {
                                    class: "discovery-row",
                                    onclick: {
                                        let c = c.clone();
                                        move |_| on_select.call(c.clone())
                                    },
                                    span { class: "discovery-name", "{c.name}" }
                                }
                            }
                        }
                    }
                }
                p { class: "discovery-hint", "Choosing a Host never grants access — the pairing code still authenticates it." }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::catalog::*;
    use super::*;

    fn txt(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn candidate_requires_macid() {
        assert!(candidate("Mac", &txt(&[])).is_none());
        assert!(candidate("Mac", &txt(&[("macid", "   ")]).clone()).is_none());
        let c = candidate("My Mac", &txt(&[("macid", "  abc-123  ")]).clone()).unwrap();
        assert_eq!(c.host_id, "abc-123");
        assert_eq!(c.name, "My Mac");
    }

    #[test]
    fn candidate_blank_name_falls_back() {
        let c = candidate("   ", &txt(&[("macid", "x")]).clone()).unwrap();
        assert_eq!(c.name, "Unpeel Host");
    }

    #[test]
    fn merging_dedups_excludes_and_sorts() {
        let cs = vec![
            NearbyHostCandidate {
                host_id: "B".into(),
                name: "zeta".into(),
            },
            NearbyHostCandidate {
                host_id: "b".into(),
                name: "ZETA-DUP".into(),
            },
            NearbyHostCandidate {
                host_id: "A".into(),
                name: "Alpha".into(),
            },
            NearbyHostCandidate {
                host_id: "SELF".into(),
                name: "Me".into(),
            },
            NearbyHostCandidate {
                host_id: "C".into(),
                name: "alpha".into(),
            },
        ];
        let merged = merging(cs, Some("self"));
        let ids: Vec<&str> = merged.iter().map(|c| c.host_id.as_str()).collect();
        // "self" excluded; "B"/"b" deduped to first-seen; sorted by
        // case-insensitive name, host id tiebreak.
        assert_eq!(ids, vec!["A", "C", "B"]);
    }

    #[test]
    fn query_packet_shape() {
        let q = build_ptr_query(MDNS_SERVICE_TYPE);
        // header (12) + labels + root + qtype/qclass (4)
        assert!(q.len() > 16);
        assert_eq!(&q[0..2], &[0x00, 0x00]);
        assert_eq!(&q[4..6], &[0x00, 0x01]); // one question
        assert_eq!(&q[q.len() - 4..], &[0x00, 0x0c, 0x00, 0x01]); // PTR/IN
    }

    fn encode_name(out: &mut Vec<u8>, name: &str) {
        for label in name.split('.') {
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
        out.push(0);
    }

    /// Build a synthetic mDNS response: one PTR + one TXT with macid.
    fn fake_response() -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&[0x00, 0x00]); // id
        p.extend_from_slice(&[0x84, 0x00]); // flags: response, authoritative
        p.extend_from_slice(&[0x00, 0x00]); // qdcount
        p.extend_from_slice(&[0x00, 0x02]); // ancount = 2
        p.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // ns/ar
                                                        // PTR _unpeel-remote._tcp.local -> MyMac._unpeel-remote._tcp.local
        encode_name(&mut p, MDNS_SERVICE_TYPE);
        p.extend_from_slice(&[0x00, 0x0c, 0x00, 0x01]); // PTR/IN
        p.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // ttl
        let mut target = Vec::new();
        encode_name(&mut target, "MyMac._unpeel-remote._tcp.local");
        p.extend_from_slice(&(target.len() as u16).to_be_bytes());
        p.extend_from_slice(&target);
        // TXT MyMac._unpeel-remote._tcp.local: macid=host-1
        encode_name(&mut p, "MyMac._unpeel-remote._tcp.local");
        p.extend_from_slice(&[0x00, 0x10, 0x00, 0x01]); // TXT/IN
        p.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // ttl
        let kv = b"macid=host-1";
        p.extend_from_slice(&((kv.len() + 1) as u16).to_be_bytes());
        p.push(kv.len() as u8);
        p.extend_from_slice(kv);
        p
    }

    #[test]
    fn parses_ptr_and_txt() {
        let parsed = parse_mdns_response(&fake_response());
        assert_eq!(parsed.len(), 1);
        let (instance, txt) = &parsed[0];
        assert_eq!(instance, "MyMac._unpeel-remote._tcp.local");
        assert_eq!(txt.get("macid").map(String::as_str), Some("host-1"));
        let c = candidate(
            instance.strip_suffix("._unpeel-remote._tcp.local").unwrap(),
            txt,
        )
        .unwrap();
        assert_eq!(c.host_id, "host-1");
        assert_eq!(c.name, "MyMac");
    }

    #[test]
    fn rejects_non_responses() {
        let mut q = build_ptr_query(MDNS_SERVICE_TYPE);
        assert!(parse_mdns_response(&q).is_empty());
        q.clear();
        assert!(parse_mdns_response(&q).is_empty());
    }

    /// Real mDNS responders often put the TXT in the additional section.
    /// The parser must collect it there too.
    #[test]
    fn parses_txt_in_additional_section() {
        let mut p = Vec::new();
        p.extend_from_slice(&[0x00, 0x00]); // id
        p.extend_from_slice(&[0x84, 0x00]); // flags: response, authoritative
        p.extend_from_slice(&[0x00, 0x00]); // qdcount
        p.extend_from_slice(&[0x00, 0x01]); // ancount = 1 (PTR)
        p.extend_from_slice(&[0x00, 0x00]); // nscount
        p.extend_from_slice(&[0x00, 0x01]); // arcount = 1 (TXT)
                                            // PTR _unpeel-remote._tcp.local -> MyMac._unpeel-remote._tcp.local
        encode_name(&mut p, MDNS_SERVICE_TYPE);
        p.extend_from_slice(&[0x00, 0x0c, 0x00, 0x01]); // PTR/IN
        p.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // ttl
        let mut target = Vec::new();
        encode_name(&mut target, "MyMac._unpeel-remote._tcp.local");
        p.extend_from_slice(&(target.len() as u16).to_be_bytes());
        p.extend_from_slice(&target);
        // TXT in the additional section.
        encode_name(&mut p, "MyMac._unpeel-remote._tcp.local");
        p.extend_from_slice(&[0x00, 0x10, 0x00, 0x01]); // TXT/IN
        p.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // ttl
        let kv = b"macid=host-9";
        p.extend_from_slice(&((kv.len() + 1) as u16).to_be_bytes());
        p.push(kv.len() as u8);
        p.extend_from_slice(kv);

        let parsed = parse_mdns_response(&p);
        assert_eq!(parsed.len(), 1);
        let (instance, txt) = &parsed[0];
        assert_eq!(instance, "MyMac._unpeel-remote._tcp.local");
        assert_eq!(txt.get("macid").map(String::as_str), Some("host-9"));
    }
}
