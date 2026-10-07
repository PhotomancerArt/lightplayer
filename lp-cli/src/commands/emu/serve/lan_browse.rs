//! `GET /lans/<name>/browse?service=<svc>[&wait_ms=<n>]`: ask a served LAN,
//! through the door's probe on it, who offers a DNS-SD service, and answer
//! with what the boards said (walk step W5).
//!
//! The probe is a participant on the segment with an address of its own
//! (`lp_emu_esp_common::seam::net::LanProbe`). The browse clears what it
//! heard before, sends a PTR question for the service to the mDNS group, and
//! asks again every second, because a board that had not joined yet cannot
//! have heard the first one. It answers once every running board on the LAN
//! has answered with an instance and its TXT record, or when `wait_ms`
//! (default 10 s) of host time is up, whichever is first. Host time, because
//! a served LAN runs on the host's clock: nothing here is deterministic, and
//! the reply is evidence of who answered, never of when.
//!
//! The reply, `application/json`:
//!
//! ```text
//! { "lan": "home", "service": "_lightplayer._tcp.local", "waited_ms": 812,
//!   "instances": [ { "instance": "lp-b48c._lightplayer._tcp.local",
//!                    "host": "lp-b48c.local", "port": 80,
//!                    "address": "192.168.4.100",
//!                    "txt": ["mac=a0f26287b48c"], "mac": "a0f26287b48c" } ],
//!   "answers": [ { "name": "…", "type": "PTR", "ttl": 4500, "data": "…" } ] }
//! ```
//!
//! `instances` is the answers read together (PTR → SRV, TXT → the target's
//! A); `answers` is every distinct record any reply carried, as heard.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use lp_emu_esp_common::seam::net::lan_dns::{
    TYPE_A, TYPE_AAAA, TYPE_ANY, TYPE_NSEC, TYPE_PTR, TYPE_SRV, TYPE_TXT,
};
use lp_emu_esp_common::seam::net::{DnsAnswer, DnsData};
use serde_json::{Value, json};

use super::served_lan::ServedLan;

/// The service a browse asks for when the query names none: the firmware's
/// (`fw-esp32-common/src/net/mdns/mdns_answer.rs`).
pub const DEFAULT_SERVICE: &str = "_lightplayer._tcp.local";

/// How long a browse waits for every board when the query names no
/// `wait_ms`.
const DEFAULT_WAIT: Duration = Duration::from_secs(10);

/// The longest `wait_ms` a query may ask for.
const MAX_WAIT: Duration = Duration::from_secs(120);

/// How often the probe's answers are looked at.
const POLL: Duration = Duration::from_millis(50);

/// How often the question is asked again while a board is still missing.
const REQUERY: Duration = Duration::from_secs(1);

/// What a browse asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowseQuery {
    pub service: String,
    pub wait: Duration,
}

/// `service=<svc>&wait_ms=<n>`, both optional, percent-decoded.
pub fn parse_query(query: &str) -> Result<BrowseQuery, String> {
    let mut out = BrowseQuery {
        service: DEFAULT_SERVICE.to_string(),
        wait: DEFAULT_WAIT,
    };
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value)?;
        match key {
            "service" => {
                let service = value.trim().trim_end_matches('.');
                if service.is_empty() {
                    return Err("service= is empty".to_string());
                }
                out.service = service.to_string();
            }
            "wait_ms" => {
                let ms: u64 = value
                    .parse()
                    .map_err(|_| format!("wait_ms=`{value}` is not a number of milliseconds"))?;
                out.wait = Duration::from_millis(ms).min(MAX_WAIT);
            }
            other => {
                return Err(format!(
                    "`{other}` is not a browse parameter (service, wait_ms)"
                ));
            }
        }
    }
    Ok(out)
}

/// Ask `lan` for `query.service` and answer once `expected` instances (with
/// their TXT) have answered, or the wait is up.
pub async fn browse(lan: &ServedLan, query: &BrowseQuery, expected: usize) -> Value {
    let _one_at_a_time = lan.browsing.lock().await;
    let probe = lan.probe();
    lan.lan.with(|l| {
        let p = l.probe_mut(probe);
        p.clear_answers();
        p.query(&query.service, TYPE_PTR);
    });
    let started = Instant::now();
    let mut asked = started;
    let answers = loop {
        tokio::time::sleep(POLL).await;
        let answers = lan.lan.with(|l| l.probe(probe).answers().to_vec());
        let complete = instances(&query.service, &answers)
            .iter()
            .filter(|i| i.txt.is_some())
            .count();
        if (expected > 0 && complete >= expected) || started.elapsed() >= query.wait {
            break answers;
        }
        if asked.elapsed() >= REQUERY {
            lan.lan
                .with(|l| l.probe_mut(probe).query(&query.service, TYPE_PTR));
            asked = Instant::now();
        }
    };
    let found = instances(&query.service, &answers);
    json!({
        "lan": lan.name,
        "service": query.service,
        "waited_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "instances": found.iter().map(Instance::to_json).collect::<Vec<_>>(),
        "answers": distinct(&answers).into_iter().map(answer_json).collect::<Vec<_>>(),
    })
}

/// One service instance, read out of the answers.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Instance {
    instance: String,
    host: Option<String>,
    port: Option<u16>,
    address: Option<Ipv4Addr>,
    txt: Option<Vec<String>>,
}

impl Instance {
    fn mac(&self) -> Option<&str> {
        self.txt
            .as_ref()?
            .iter()
            .find_map(|entry| entry.strip_prefix("mac="))
    }

    fn to_json(&self) -> Value {
        json!({
            "instance": self.instance,
            "host": self.host,
            "port": self.port,
            "address": self.address.map(|ip| ip.to_string()),
            "txt": self.txt,
            "mac": self.mac(),
        })
    }
}

/// Every instance a PTR for `service` names, with its SRV, TXT and the
/// target's A where the answers carry them. In first-heard order.
fn instances(service: &str, answers: &[DnsAnswer]) -> Vec<Instance> {
    let mut out: Vec<Instance> = Vec::new();
    for answer in answers {
        if let DnsData::Ptr(instance) = &answer.data
            && answer.is_named(service)
            && !out
                .iter()
                .any(|i| i.instance.eq_ignore_ascii_case(instance))
        {
            out.push(Instance {
                instance: instance.clone(),
                host: None,
                port: None,
                address: None,
                txt: None,
            });
        }
    }
    for found in &mut out {
        for answer in answers.iter().filter(|a| a.is_named(&found.instance)) {
            match &answer.data {
                DnsData::Srv { port, target, .. } => {
                    found.host.get_or_insert_with(|| target.clone());
                    found.port.get_or_insert(*port);
                }
                DnsData::Txt(entries) => {
                    found.txt.get_or_insert_with(|| {
                        entries
                            .iter()
                            .map(|e| String::from_utf8_lossy(e).into_owned())
                            .collect()
                    });
                }
                _ => {}
            }
        }
        if let Some(host) = &found.host {
            found.address = answers.iter().find_map(|a| match a.data {
                DnsData::A(ip) if a.is_named(host) => Some(ip),
                _ => None,
            });
        }
    }
    out
}

/// The answers with repeats (the same record from a second reply) left out.
fn distinct(answers: &[DnsAnswer]) -> Vec<&DnsAnswer> {
    let mut out: Vec<&DnsAnswer> = Vec::new();
    for answer in answers {
        if !out.iter().any(|seen| {
            seen.name.eq_ignore_ascii_case(&answer.name)
                && seen.rtype == answer.rtype
                && seen.data == answer.data
        }) {
            out.push(answer);
        }
    }
    out
}

fn answer_json(answer: &DnsAnswer) -> Value {
    let data = match &answer.data {
        DnsData::A(ip) => json!(ip.to_string()),
        DnsData::Ptr(name) => json!(name),
        DnsData::Srv {
            priority,
            weight,
            port,
            target,
        } => json!({ "priority": priority, "weight": weight, "port": port, "target": target }),
        DnsData::Txt(entries) => json!(
            entries
                .iter()
                .map(|e| String::from_utf8_lossy(e).into_owned())
                .collect::<Vec<_>>()
        ),
        DnsData::Other(bytes) => {
            json!(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
        }
    };
    json!({
        "name": answer.name,
        "type": type_name(answer.rtype),
        "ttl": answer.ttl,
        "data": data,
    })
}

fn type_name(rtype: u16) -> String {
    match rtype {
        TYPE_A => "A".to_string(),
        TYPE_PTR => "PTR".to_string(),
        TYPE_TXT => "TXT".to_string(),
        TYPE_AAAA => "AAAA".to_string(),
        TYPE_SRV => "SRV".to_string(),
        TYPE_NSEC => "NSEC".to_string(),
        TYPE_ANY => "ANY".to_string(),
        other => other.to_string(),
    }
}

/// `%5F` → `_`, and `+` stays `+` (a service name has none). Enough for a
/// query string a page or `curl` builds with `encodeURIComponent`.
fn percent_decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| format!("`{text}` has a bad percent escape"))?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| format!("`{text}` is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_names_a_service_and_a_wait_or_takes_the_defaults() {
        assert_eq!(
            parse_query("").unwrap(),
            BrowseQuery {
                service: DEFAULT_SERVICE.to_string(),
                wait: DEFAULT_WAIT
            }
        );
        let q = parse_query("service=_lightplayer._tcp.local.&wait_ms=500").unwrap();
        assert_eq!(q.service, "_lightplayer._tcp.local");
        assert_eq!(q.wait, Duration::from_millis(500));
        assert_eq!(
            parse_query("service=%5Flightplayer%2E_tcp.local")
                .unwrap()
                .service,
            "_lightplayer._tcp.local"
        );
        assert_eq!(parse_query("wait_ms=99999999").unwrap().wait, MAX_WAIT);
        assert!(parse_query("wait_ms=soon").is_err());
        assert!(parse_query("service=").is_err());
        assert!(parse_query("colour=blue").is_err());
        assert!(parse_query("service=%zz").is_err());
    }

    #[test]
    fn the_answers_read_together_into_one_instance_per_board() {
        let service = DEFAULT_SERVICE;
        let a = |name: &str, data: DnsData, rtype: u16| DnsAnswer {
            name: name.to_string(),
            rtype,
            ttl: 120,
            data,
        };
        let answers = vec![
            a(
                service,
                DnsData::Ptr("lp-b48c._lightplayer._tcp.local".into()),
                TYPE_PTR,
            ),
            a(
                "lp-b48c._lightplayer._tcp.local",
                DnsData::Srv {
                    priority: 0,
                    weight: 0,
                    port: 80,
                    target: "lp-b48c.local".into(),
                },
                TYPE_SRV,
            ),
            a(
                "lp-b48c._lightplayer._tcp.local",
                DnsData::Txt(vec![b"mac=a0f26287b48c".to_vec()]),
                TYPE_TXT,
            ),
            a(
                "lp-b48c.local",
                DnsData::A(Ipv4Addr::new(192, 168, 4, 100)),
                TYPE_A,
            ),
            // The second board, which has not sent its TXT yet.
            a(
                service,
                DnsData::Ptr("lp-0001._lightplayer._tcp.local".into()),
                TYPE_PTR,
            ),
            // A repeat from a second reply.
            a(
                service,
                DnsData::Ptr("lp-b48c._lightplayer._tcp.local".into()),
                TYPE_PTR,
            ),
        ];
        let found = instances(service, &answers);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].host.as_deref(), Some("lp-b48c.local"));
        assert_eq!(found[0].port, Some(80));
        assert_eq!(found[0].address, Some(Ipv4Addr::new(192, 168, 4, 100)));
        assert_eq!(found[0].mac(), Some("a0f26287b48c"));
        assert_eq!(found[1].txt, None, "not complete until its TXT is heard");
        assert_eq!(distinct(&answers).len(), 5);
        let shown = answer_json(&answers[2]);
        assert_eq!(shown["type"], "TXT");
        assert_eq!(shown["data"][0], "mac=a0f26287b48c");
    }
}
