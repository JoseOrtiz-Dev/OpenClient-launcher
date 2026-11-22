use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MojangSkin {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub variant: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MojangCape {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub alias: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MinecraftAccount {
    pub id: String,
    pub username: String,
    pub uuid: String,
    pub account_type: String, // "microsoft" | "offline"
    pub access_token: String,
    pub skin_url: String,
    #[serde(default)]
    pub skin_variant: String,
    pub cape_id: Option<String>,
    pub capes: Vec<MojangCape>,
    #[serde(default)]
    pub expires_at: u64,
}

pub fn generate_offline_uuid(username: &str) -> String {
    use md5::{Digest, Md5};
    let input = format!("OfflinePlayer:{}", username);
    let digest: [u8; 16] = Md5::digest(input.as_bytes()).into();
    uuid::Builder::from_md5_bytes(digest)
        .into_uuid()
        .to_string()
}

pub fn login_offline(username: String) -> Result<MinecraftAccount, String> {
    let trimmed = username.trim();
    if trimmed.len() < 3
        || trimmed.len() > 16
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("El usuario debe tener entre 3 y 16 caracteres.".into());
    }

    let uuid = generate_offline_uuid(trimmed);
    Ok(MinecraftAccount {
        id: format!("offline-{}", uuid),
        username: trimmed.to_string(),
        uuid,
        account_type: "offline".to_string(),
        access_token: "0".to_string(),
        skin_url: format!("https://mc-heads.net/skin/{}", trimmed),
        skin_variant: "classic".to_string(),
        cape_id: None,
        capes: vec![],
        expires_at: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_is_the_deterministic_version_3_hash_of_the_player_name() {
        // Valor conocido de Minecraft: UUID.nameUUIDFromBytes("OfflinePlayer:Notch").
        assert_eq!(generate_offline_uuid("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(generate_offline_uuid("Notch"), generate_offline_uuid("Notch"));
        // Distingue mayúsculas, como el servidor.
        assert_ne!(generate_offline_uuid("notch"), generate_offline_uuid("Notch"));
        for name in ["Steve", "Alex_01", "abc", "A_B_C_D_E_F_G_H_"] {
            let uuid = uuid::Uuid::parse_str(&generate_offline_uuid(name)).unwrap();
            assert_eq!(uuid.get_version_num(), 3, "{name}");
            assert_eq!(uuid.get_variant(), uuid::Variant::RFC4122, "{name}");
        }
    }

    #[test]
    fn offline_login_trims_the_name_and_builds_a_local_account() {
        let account = login_offline("  Steve_01 \n".into()).unwrap();
        assert_eq!(account.username, "Steve_01");
        assert_eq!(account.uuid, generate_offline_uuid("Steve_01"));
        assert_eq!(account.id, format!("offline-{}", account.uuid));
        assert_eq!(account.account_type, "offline");
        assert_eq!(account.access_token, "0");
        assert_eq!(account.skin_url, "https://mc-heads.net/skin/Steve_01");
        assert_eq!(account.skin_variant, "classic");
        assert!(account.cape_id.is_none() && account.capes.is_empty());
        assert_eq!(account.expires_at, 0);
        // Los límites 3 y 16 son válidos.
        assert!(login_offline("abc".into()).is_ok());
        assert!(login_offline("a".repeat(16)).is_ok());
        // La misma persona obtiene la misma cuenta cada vez.
        assert_eq!(login_offline("Steve_01".into()).unwrap().id, account.id);
    }

    #[test]
    fn invalid_offline_names_are_rejected_with_the_same_message() {
        let too_long = "a".repeat(17);
        for bad in [
            "", "   ", "ab", " ab ", too_long.as_str(), "con espacio", "guion-medio", "punto.punto",
            "ñandú", "名前名前", "emoji😀x", "tab\tname", "a;b", "../x", "name\0",
        ] {
            assert_eq!(
                login_offline(bad.into()).unwrap_err(),
                "El usuario debe tener entre 3 y 16 caracteres.",
                "{bad:?}"
            );
        }
    }
}
