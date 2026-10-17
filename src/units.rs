//! Arma 3 Units, the Bohemia community groups the user belongs to (`UnitsService`).

use std::net::Ipv4Addr;

use serde::Deserialize;

const API: &str = "https://account.bistudio.com/api/v1.0";
/// The launcher's API identity: without it, and the Origin header, the API answers 401.
const QUERY: &str = "client_id=$d272a20c53fdf1be0813efe8b764b58&app_id=SPROCKET2ARMA3ALPHA001";
const ORIGIN: &str = "https://launcher.bistudio.com:666";

#[derive(Clone, Debug)]
pub enum Units {
    /// The Steam account isn't linked to a Bohemia account (the API answers 404).
    NotConnected,
    Failed,
    Found(Vec<Unit>),
}

impl Units {
    /// Every unit's servers (connection endpoints), in unit order.
    pub fn servers(&self) -> Vec<(Ipv4Addr, u16)> {
        match self {
            Units::Found(units) => units.iter().flat_map(|u| u.servers.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// The unit a server (address, game port) belongs to (`GetUnitForServer`).
    pub fn unit_for(&self, addr: Ipv4Addr, port: u16) -> Option<&Unit> {
        match self {
            Units::Found(units) => units.iter().find(|u| u.servers.contains(&(addr, port))),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Unit {
    pub name: String,
    pub profile_url: String,
    pub logo_url: Option<String>,
    /// Game servers: address and connection port.
    pub servers: Vec<(Ipv4Addr, u16)>,
}

#[derive(Deserialize)]
struct StaticData {
    assets: Assets,
}

#[derive(Deserialize)]
struct Assets {
    #[serde(rename = "groupProfileUrl")]
    group_profile_url: String,
    images: Images,
}

#[derive(Deserialize)]
struct Images {
    #[serde(rename = "baseUrl")]
    base_url: String,
}

#[derive(Deserialize)]
struct Group {
    id: serde_json::Value,
    slug: String,
    name: String,
    #[serde(default)]
    logo_img_name: Option<String>,
    #[serde(default)]
    servers: Vec<GroupServer>,
}

#[derive(Deserialize)]
struct GroupServer {
    ip_address: String,
    /// A number or a numeric string.
    port: serde_json::Value,
}

impl GroupServer {
    fn endpoint(&self) -> Option<(Ipv4Addr, u16)> {
        let port = match &self.port {
            serde_json::Value::String(s) => s.trim().parse().ok()?,
            serde_json::Value::Number(n) => u16::try_from(n.as_u64()?).ok()?,
            _ => return None,
        };
        Some((self.ip_address.trim().parse().ok()?, port))
    }
}

fn get(path: &str) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    ureq::get(&format!("{API}{path}?{QUERY}"))
        .header("Origin", ORIGIN)
        .header("User-Agent", "Arma 3 Launcher")
        .config()
        .http_status_as_error(false)
        .build()
        .call()
}

/// The units of the Steam user `steam_id` (a blocking request; run it off the UI thread).
pub fn fetch(steam_id: u64) -> Units {
    let Ok(mut response) = get("/group/data") else {
        return Units::Failed;
    };
    let Ok(data) = response.body_mut().read_json::<StaticData>() else {
        return Units::Failed;
    };
    let Ok(mut response) = get(&format!("/user/steam/{steam_id}/group")) else {
        return Units::Failed;
    };
    match response.status().as_u16() {
        404 => return Units::NotConnected,
        200 => {}
        _ => return Units::Failed,
    }
    let Ok(groups) = response.body_mut().read_json::<Vec<Group>>() else {
        return Units::Failed;
    };
    let units = groups
        .into_iter()
        .map(|g| {
            let id = match &g.id {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            Unit {
                profile_url: data.assets.group_profile_url.replace(":groupSlug", &g.slug),
                logo_url: g.logo_img_name.filter(|l| !l.is_empty()).map(|logo| {
                    format!(
                        "{}{logo}",
                        data.assets.images.base_url.replace(":groupID", &id)
                    )
                }),
                servers: g.servers.iter().filter_map(GroupServer::endpoint).collect(),
                name: g.name,
            }
        })
        .collect();
    Units::Found(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_servers_take_numeric_or_text_ports() {
        let group: Group = serde_json::from_str(
            r#"{"id":7,"slug":"tbd","name":"TBD","servers":[
                {"ip_address":"1.2.3.4","port":2302},
                {"ip_address":"5.6.7.8","port":"2402"},
                {"ip_address":"not an ip","port":2302}]}"#,
        )
        .unwrap();
        let servers: Vec<_> = group
            .servers
            .iter()
            .filter_map(GroupServer::endpoint)
            .collect();
        assert_eq!(
            servers,
            [
                (Ipv4Addr::new(1, 2, 3, 4), 2302),
                (Ipv4Addr::new(5, 6, 7, 8), 2402)
            ]
        );
    }
}
