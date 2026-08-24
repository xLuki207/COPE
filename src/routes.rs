use crate::parser::SolanaAddress;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Destination {
    Axiom,
    Gmgn,
    XSearch,
    DexScreener,
    Pumpfun,
    Fomo,
    Solscan,
    RugCheck,
    BundleChecker,
}

impl Destination {
    pub fn all() -> &'static [Destination] {
        &[
            Destination::Axiom,
            Destination::Gmgn,
            Destination::XSearch,
            Destination::DexScreener,
            Destination::Pumpfun,
            Destination::Fomo,
            Destination::Solscan,
            Destination::RugCheck,
            Destination::BundleChecker,
        ]
    }

    pub fn default_hotkey(&self) -> Option<(&'static str, u32, u32)> {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        match self {
            Destination::Axiom => Some(("Alt+A", MOD_ALT.0, u32::from(VK_A.0))),
            Destination::Gmgn => Some(("Alt+G", MOD_ALT.0, u32::from(VK_G.0))),
            Destination::XSearch => Some(("Alt+X", MOD_ALT.0, u32::from(VK_X.0))),
            Destination::DexScreener => Some(("Alt+D", MOD_ALT.0, u32::from(VK_D.0))),
            Destination::Pumpfun => Some(("Alt+P", MOD_ALT.0, u32::from(VK_P.0))),
            Destination::Fomo => Some(("Alt+F", MOD_ALT.0, u32::from(VK_F.0))),
            Destination::Solscan => Some(("Alt+S", MOD_ALT.0, u32::from(VK_S.0))),
            Destination::RugCheck => Some(("Alt+Q", MOD_ALT.0, u32::from(VK_Q.0))),
            Destination::BundleChecker => Some(("Alt+B", MOD_ALT.0, u32::from(VK_B.0))),
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Destination::Axiom => "Axiom",
            Destination::Gmgn => "GMGN",
            Destination::XSearch => "X Search",
            Destination::DexScreener => "DexScreener",
            Destination::Pumpfun => "Pump.fun",
            Destination::Fomo => "FOMO",
            Destination::Solscan => "Solscan",
            Destination::RugCheck => "RugCheck",
            Destination::BundleChecker => "Bundle Checker",
        }
    }

    pub fn build_url(&self, address: &SolanaAddress) -> String {
        let addr = address.as_str();
        match self {
            Destination::Axiom => format!("https://axiom.trade/t/{}?chain=sol", addr),
            Destination::Gmgn => format!("https://gmgn.ai/sol/token/{}", addr),
            Destination::XSearch => format!("https://x.com/search?q={}&src=typed_query", addr),
            Destination::DexScreener => format!("https://dexscreener.com/solana/{}", addr),
            Destination::Pumpfun => format!("https://pump.fun/coin/{}", addr),
            Destination::Fomo => format!("https://fomo.family/tokens/solana/{}", addr),
            Destination::Solscan => format!("https://solscan.io/token/{}", addr),
            Destination::RugCheck => format!("https://rugcheck.xyz/tokens/{}", addr),
            Destination::BundleChecker => format!("https://trench.bot/clusters/{}", addr),
        }
    }

    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "axiom" => Some(Destination::Axiom),
            "gmgn" => Some(Destination::Gmgn),
            "xsearch" | "x_search" | "x" => Some(Destination::XSearch),
            "dexscreener" | "dex_screener" => Some(Destination::DexScreener),
            "photon" => Some(Destination::Pumpfun),
            "fomo" => Some(Destination::Fomo),
            "solscan" => Some(Destination::Solscan),
            "rugcheck" => Some(Destination::RugCheck),
            "bundlechecker" | "bundle_checker" | "bundle" => Some(Destination::BundleChecker),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConfig {
    pub destination: Destination,
    pub modifiers: u32,
    pub vk_code: u32,
    pub enabled: bool,
}

const MOD_ALT: u32 = 0x0001;
const MOD_CTRL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;

impl RouteConfig {
    pub fn default_for(destination: Destination) -> Self {
        let (_, mods, vk) = destination.default_hotkey().unwrap();
        Self {
            destination,
            modifiers: mods,
            vk_code: vk,
            enabled: true,
        }
    }

    pub fn hotkey_string(&self) -> String {
        let mut mods = String::new();
        if self.modifiers & MOD_CTRL != 0 {
            mods.push_str("Ctrl+");
        }
        if self.modifiers & MOD_SHIFT != 0 {
            mods.push_str("Shift+");
        }
        if self.modifiers & MOD_ALT != 0 {
            mods.push_str("Alt+");
        }
        format!("{}{}", mods, vk_to_string(self.vk_code))
    }
}

fn vk_to_string(vk: u32) -> String {
    match vk {
        0x41..=0x5A => char::from_u32(vk).unwrap().to_string(),
        0x30..=0x39 => char::from_u32(vk).unwrap().to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        _ => format!("VK_{:02X}", vk),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutesConfig {
    pub routes: HashMap<Destination, RouteConfig>,
}

impl Default for RoutesConfig {
    fn default() -> Self {
        let mut routes = HashMap::new();
        for dest in Destination::all() {
            routes.insert(*dest, RouteConfig::default_for(*dest));
        }
        Self { routes }
    }
}

impl RoutesConfig {
    pub fn get(&self, destination: Destination) -> Option<&RouteConfig> {
        self.routes.get(&destination)
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, destination: Destination) -> Option<&mut RouteConfig> {
        self.routes.get_mut(&destination)
    }

    pub fn enabled_routes(&self) -> Vec<(&Destination, &RouteConfig)> {
        self.routes.iter().filter(|(_, rc)| rc.enabled).collect()
    }
}

pub fn open_destination(destination: Destination, address: &SolanaAddress) -> Result<()> {
    let url = destination.build_url(address);
    open::that(url)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CA_1: &str = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";

    #[test]
    fn test_route_urls() {
        let addr = SolanaAddress(VALID_CA_1.to_string());

        assert_eq!(
            Destination::Axiom.build_url(&addr),
            format!("https://axiom.trade/t/{}?chain=sol", VALID_CA_1)
        );
        assert_eq!(
            Destination::Gmgn.build_url(&addr),
            format!("https://gmgn.ai/sol/token/{}", VALID_CA_1)
        );
        assert_eq!(
            Destination::XSearch.build_url(&addr),
            format!("https://x.com/search?q={}&src=typed_query", VALID_CA_1)
        );
        assert_eq!(
            Destination::DexScreener.build_url(&addr),
            format!("https://dexscreener.com/solana/{}", VALID_CA_1)
        );
        assert_eq!(
            Destination::Pumpfun.build_url(&addr),
            format!("https://pump.fun/coin/{}", VALID_CA_1)
        );
        assert_eq!(
            Destination::Fomo.build_url(&addr),
            format!("https://fomo.family/tokens/solana/{}", VALID_CA_1)
        );
        assert_eq!(
            Destination::Solscan.build_url(&addr),
            format!("https://solscan.io/token/{}", VALID_CA_1)
        );
        assert_eq!(
            Destination::RugCheck.build_url(&addr),
            format!("https://rugcheck.xyz/tokens/{}", VALID_CA_1)
        );
        assert_eq!(
            Destination::BundleChecker.build_url(&addr),
            format!("https://trench.bot/clusters/{}", VALID_CA_1)
        );
    }

    #[test]
    fn test_from_str() {
        assert_eq!(Destination::from_str("axiom"), Some(Destination::Axiom));
        assert_eq!(Destination::from_str("gmgn"), Some(Destination::Gmgn));
        assert_eq!(Destination::from_str("xsearch"), Some(Destination::XSearch));
        assert_eq!(
            Destination::from_str("x_search"),
            Some(Destination::XSearch)
        );
        assert_eq!(
            Destination::from_str("dexscreener"),
            Some(Destination::DexScreener)
        );
        assert_eq!(Destination::from_str("photon"), Some(Destination::Pumpfun));
        assert_eq!(Destination::from_str("fomo"), Some(Destination::Fomo));
        assert_eq!(Destination::from_str("solscan"), Some(Destination::Solscan));
        assert_eq!(
            Destination::from_str("rugcheck"),
            Some(Destination::RugCheck)
        );
        assert_eq!(
            Destination::from_str("bundlechecker"),
            Some(Destination::BundleChecker)
        );
        assert_eq!(Destination::from_str("unknown"), None);
        assert_eq!(
            Destination::from_str("bundle"),
            Some(Destination::BundleChecker)
        );
    }

    #[test]
    fn test_default_hotkeys() {
        assert_eq!(
            Destination::Axiom.default_hotkey(),
            Some(("Alt+A", 0x0001, 0x41))
        );
        assert_eq!(
            Destination::Gmgn.default_hotkey(),
            Some(("Alt+G", 0x0001, 0x47))
        );
        assert_eq!(
            Destination::XSearch.default_hotkey(),
            Some(("Alt+X", 0x0001, 0x58))
        );
        assert_eq!(
            Destination::DexScreener.default_hotkey(),
            Some(("Alt+D", 0x0001, 0x44))
        );
        assert_eq!(
            Destination::Pumpfun.default_hotkey(),
            Some(("Alt+P", 0x0001, 0x50))
        );
        assert_eq!(
            Destination::Fomo.default_hotkey(),
            Some(("Alt+F", 0x0001, 0x46))
        );
        assert_eq!(
            Destination::Solscan.default_hotkey(),
            Some(("Alt+S", 0x0001, 0x53))
        );
        assert_eq!(
            Destination::RugCheck.default_hotkey(),
            Some(("Alt+Q", 0x0001, 0x51))
        );
    }

    #[test]
    fn test_axiom_and_rugcheck_urls_with_10_distinct_valid_cas() {
        let cas = [
            "5vhw96LZoKA4K8kbBkTJXBYvJSN1RWJzJgJbYEorBChv",
            "A9ecbzftsKGF99cNf3bWxv1HvGt3zrG8hC2v7CaGn5Br",
            "sokhCSmzutMPPuNcxG1j6gYLowgiM8mswjJu8FBYm5r",
            "BCWd9Gw3dGzjLiTqX3xnSFNJG8Jywrc7wUDtXW3E9F5n",
            "ALAcK3DshitVofFY8xRc6cR4W9Ywo1f2GDG95UcZUijd",
            "SooEj828BSjtgTecBRkqBJ4oquc713yyFZqbCawawoN",
            "DWCj1WhLQouv7WgfzLM4V1K3AQadZr4re6gSbLdD7Ppo",
            "sosd5Q3DutGxMEaukBDmkPgsapMQz59jNjGWmhYcdTQ",
            "7AEEdoP2zhMad1tkFNYyDnDB9Hjy7sb1LSB2KxAQr3EV",
            "5XXn2PCJDCGUViEUcwKA6gLcTS5zG5ESnmTsfvEvDU4P",
        ];

        for ca in &cas {
            assert!(matches!(
                crate::parser::extract_solana_addresses(ca),
                Ok(crate::parser::ExtractResult::Single(_))
            ));
            let addr = SolanaAddress(ca.to_string());
            let url = Destination::Axiom.build_url(&addr);
            assert_eq!(url, format!("https://axiom.trade/t/{}?chain=sol", ca));
            assert!(url.starts_with("https://axiom.trade/t/"));
            assert!(url.ends_with("?chain=sol"));
            assert!(!url.contains("/meme/"));
            assert!(!url.contains("pulseChains"));
            assert!(!url.contains("trackerChains"));

            let rugcheck = Destination::RugCheck.build_url(&addr);
            assert_eq!(rugcheck, format!("https://rugcheck.xyz/tokens/{}", ca));
        }
    }

    #[test]
    fn test_axiom_stale_ca_not_reused() {
        let url_a = Destination::Axiom.build_url(&SolanaAddress(
            "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU".to_string(),
        ));
        let url_b = Destination::Axiom.build_url(&SolanaAddress(
            "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM".to_string(),
        ));
        assert_ne!(url_a, url_b);
        assert!(url_a.contains("7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"));
        assert!(url_b.contains("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"));
    }

    #[test]
    fn test_all_returns_all_destinations() {
        let all = Destination::all();
        assert_eq!(all.len(), 9);
        assert!(all.contains(&Destination::Axiom));
        assert!(all.contains(&Destination::Gmgn));
        assert!(all.contains(&Destination::XSearch));
        assert!(all.contains(&Destination::DexScreener));
        assert!(all.contains(&Destination::Pumpfun));
        assert!(all.contains(&Destination::Fomo));
        assert!(all.contains(&Destination::Solscan));
        assert!(all.contains(&Destination::RugCheck));
        assert!(all.contains(&Destination::BundleChecker));
    }
}
