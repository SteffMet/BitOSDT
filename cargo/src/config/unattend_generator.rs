use crate::core::errors::{BitOSDTError, BitOSDTResult};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Unattend.xml generator for Windows automated installation
pub struct UnattendGenerator;

/// Complete unattend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnattendConfig {
    /// Regional settings
    pub language: String,
    pub input_locale: String,
    pub timezone: String,

    /// OOBE skip options
    pub oobe: OobeConfig,

    /// User accounts to create
    pub users: Vec<UserAccountConfig>,

    /// Built-in Administrator password (optional)
    pub administrator_password: Option<String>,

    /// Computer name (supports patterns like %SERIAL%, %RANDOM%)
    pub computer_name: Option<String>,

    /// Product key (optional)
    pub product_key: Option<String>,

    /// Domain join configuration (optional)
    pub domain_join: Option<DomainJoinConfig>,

    /// Wi-Fi profile for automatic wireless connection (optional)
    pub wifi_profile: Option<WifiProfileConfig>,

    /// Auto-logon configuration (optional)
    pub auto_logon: Option<AutoLogonConfig>,

    /// First logon commands
    pub first_logon_commands: Vec<FirstLogonCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OobeConfig {
    pub skip_machine_oobe: bool,
    pub skip_user_oobe: bool,
    pub hide_eula: bool,
    pub hide_wireless_setup: bool,
    pub hide_local_account_screen: bool,
    pub hide_online_account_screens: bool,
    pub network_location: NetworkLocation,
    pub protect_your_pc: ProtectYourPc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkLocation {
    Home,
    Work,
    Other,
}

impl NetworkLocation {
    fn as_str(&self) -> &'static str {
        match self {
            NetworkLocation::Home => "Home",
            NetworkLocation::Work => "Work",
            NetworkLocation::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtectYourPc {
    Recommended,
    Custom,
    Off,
}

impl ProtectYourPc {
    fn as_value(&self) -> &'static str {
        match self {
            ProtectYourPc::Recommended => "1",
            ProtectYourPc::Custom => "2",
            ProtectYourPc::Off => "3",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccountConfig {
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
    pub group: UserGroup,
    pub password_never_expires: bool,
    pub require_password_change: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserGroup {
    Administrators,
    Users,
}

impl UserGroup {
    fn as_str(&self) -> &'static str {
        match self {
            UserGroup::Administrators => "Administrators",
            UserGroup::Users => "Users",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainJoinConfig {
    pub domain: String,
    pub username: String,
    pub password: String,
    pub ou_path: Option<String>,
    pub machine_object_ou: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiProfileConfig {
    pub ssid: String,
    pub password: String,
    pub authentication: WifiAuthentication,
    pub encryption: WifiEncryption,
    pub auto_connect: bool,
    pub hidden_network: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WifiAuthentication {
    Open,
    Wpa2Psk,
    Wpa3Sae,
}

impl WifiAuthentication {
    fn as_str(&self) -> &'static str {
        match self {
            WifiAuthentication::Open => "open",
            WifiAuthentication::Wpa2Psk => "WPA2PSK",
            WifiAuthentication::Wpa3Sae => "WPA3SAE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WifiEncryption {
    None,
    Aes,
    Tkip,
}

impl WifiEncryption {
    fn as_str(&self) -> &'static str {
        match self {
            WifiEncryption::None => "none",
            WifiEncryption::Aes => "AES",
            WifiEncryption::Tkip => "TKIP",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoLogonConfig {
    pub username: String,
    pub password: String,
    pub domain: Option<String>,
    pub logon_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstLogonCommand {
    pub order: u32,
    pub command_line: String,
    pub description: String,
    pub require_input: bool,
}

impl Default for OobeConfig {
    fn default() -> Self {
        Self {
            skip_machine_oobe: true,
            skip_user_oobe: true,
            hide_eula: true,
            hide_wireless_setup: true,
            hide_local_account_screen: false,
            hide_online_account_screens: true,
            network_location: NetworkLocation::Work,
            protect_your_pc: ProtectYourPc::Recommended,
        }
    }
}

impl Default for UnattendConfig {
    fn default() -> Self {
        Self {
            language: "en-US".to_string(),
            input_locale: "0409:00000409".to_string(),
            timezone: "Pacific Standard Time".to_string(),
            oobe: OobeConfig::default(),
            users: vec![],
            administrator_password: None,
            computer_name: None,
            product_key: None,
            domain_join: None,
            wifi_profile: None,
            auto_logon: None,
            first_logon_commands: vec![],
        }
    }
}

impl UnattendGenerator {
    /// Generate complete unattend.xml
    pub fn generate(config: &UnattendConfig) -> BitOSDTResult<String> {
        info!("Generating unattend.xml");

        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="utf-8"?>"#);
        xml.push('\n');
        xml.push_str(r#"<unattend xmlns="urn:schemas-microsoft-com:unattend">"#);
        xml.push('\n');

        // WindowsPE pass - international settings
        xml.push_str(&Self::generate_windowspe_pass(config));

        // Specialize pass - computer name, domain join
        xml.push_str(&Self::generate_specialize_pass(config));

        // OOBE System pass - user accounts, OOBE settings
        xml.push_str(&Self::generate_oobe_pass(config));

        xml.push_str("</unattend>\n");

        // Validate XML before returning
        Self::validate_xml(&xml)?;

        Ok(xml)
    }

    fn generate_windowspe_pass(config: &UnattendConfig) -> String {
        let mut xml = String::new();
        xml.push_str(r#"    <settings pass="windowsPE">"#);
        xml.push('\n');

        // International settings for WinPE
        xml.push_str(r#"        <component name="Microsoft-Windows-International-Core-WinPE" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#);
        xml.push('\n');
        xml.push_str("            <SetupUILanguage>\n");
        xml.push_str(&format!(
            "                <UILanguage>{}</UILanguage>\n",
            config.language
        ));
        xml.push_str("            </SetupUILanguage>\n");
        xml.push_str(&format!(
            "            <InputLocale>{}</InputLocale>\n",
            config.input_locale
        ));
        xml.push_str(&format!(
            "            <SystemLocale>{}</SystemLocale>\n",
            config.language
        ));
        xml.push_str(&format!(
            "            <UILanguage>{}</UILanguage>\n",
            config.language
        ));
        xml.push_str(&format!(
            "            <UserLocale>{}</UserLocale>\n",
            config.language
        ));
        xml.push_str("        </component>\n");

        // Setup component with product key
        xml.push_str(r#"        <component name="Microsoft-Windows-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#);
        xml.push('\n');

        if let Some(ref key) = config.product_key {
            xml.push_str("            <UserData>\n");
            xml.push_str("                <AcceptEula>true</AcceptEula>\n");
            xml.push_str("                <ProductKey>\n");
            xml.push_str(&format!("                    <Key>{}</Key>\n", key));
            xml.push_str("                </ProductKey>\n");
            xml.push_str("            </UserData>\n");
        } else {
            xml.push_str("            <UserData>\n");
            xml.push_str("                <AcceptEula>true</AcceptEula>\n");
            xml.push_str("            </UserData>\n");
        }

        xml.push_str("        </component>\n");
        xml.push_str("    </settings>\n");

        xml
    }

    fn generate_specialize_pass(config: &UnattendConfig) -> String {
        let mut xml = String::new();
        xml.push_str(r#"    <settings pass="specialize">"#);
        xml.push('\n');

        // Shell setup - computer name, timezone
        xml.push_str(r#"        <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#);
        xml.push('\n');

        if let Some(ref computer_name) = config.computer_name {
            xml.push_str(&format!(
                "            <ComputerName>{}</ComputerName>\n",
                computer_name
            ));
        } else {
            xml.push_str("            <ComputerName>*</ComputerName>\n");
        }

        xml.push_str(&format!(
            "            <TimeZone>{}</TimeZone>\n",
            config.timezone
        ));
        xml.push_str("        </component>\n");

        // Domain join (if configured)
        if let Some(ref domain) = config.domain_join {
            xml.push_str(&Self::generate_domain_join_component(domain));
        }

        xml.push_str("    </settings>\n");
        xml
    }

    fn generate_domain_join_component(domain: &DomainJoinConfig) -> String {
        let mut xml = String::new();
        xml.push_str(r#"        <component name="Microsoft-Windows-UnattendedJoin" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#);
        xml.push('\n');
        xml.push_str("            <Identification>\n");
        xml.push_str(&format!(
            "                <JoinDomain>{}</JoinDomain>\n",
            domain.domain
        ));

        if let Some(ref ou) = domain.machine_object_ou {
            xml.push_str(&format!(
                "                <MachineObjectOU>{}</MachineObjectOU>\n",
                ou
            ));
        }

        xml.push_str("                <Credentials>\n");

        // Extract domain from username if in DOMAIN\user format
        let (cred_domain, username) = if domain.username.contains('\\') {
            let parts: Vec<&str> = domain.username.splitn(2, '\\').collect();
            (parts[0].to_string(), parts[1].to_string())
        } else {
            (
                domain
                    .domain
                    .split('.')
                    .next()
                    .unwrap_or(&domain.domain)
                    .to_string(),
                domain.username.clone(),
            )
        };

        xml.push_str(&format!(
            "                    <Domain>{}</Domain>\n",
            cred_domain
        ));
        xml.push_str(&format!(
            "                    <Password>{}</Password>\n",
            Self::encode_password(&domain.password)
        ));
        xml.push_str(&format!(
            "                    <Username>{}</Username>\n",
            username
        ));
        xml.push_str("                </Credentials>\n");
        xml.push_str("            </Identification>\n");
        xml.push_str("        </component>\n");
        xml
    }

    fn generate_oobe_pass(config: &UnattendConfig) -> String {
        let mut xml = String::new();
        xml.push_str(r#"    <settings pass="oobeSystem">"#);
        xml.push('\n');

        // Shell setup component
        xml.push_str(r#"        <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#);
        xml.push('\n');

        // OOBE settings
        xml.push_str("            <OOBE>\n");
        xml.push_str(&format!(
            "                <HideEULAPage>{}</HideEULAPage>\n",
            config.oobe.hide_eula
        ));
        xml.push_str(&format!(
            "                <HideWirelessSetupInOOBE>{}</HideWirelessSetupInOOBE>\n",
            config.oobe.hide_wireless_setup
        ));
        xml.push_str(&format!(
            "                <HideLocalAccountScreen>{}</HideLocalAccountScreen>\n",
            config.oobe.hide_local_account_screen
        ));
        xml.push_str(&format!(
            "                <HideOnlineAccountScreens>{}</HideOnlineAccountScreens>\n",
            config.oobe.hide_online_account_screens
        ));
        xml.push_str(&format!(
            "                <NetworkLocation>{}</NetworkLocation>\n",
            config.oobe.network_location.as_str()
        ));
        xml.push_str(&format!(
            "                <ProtectYourPC>{}</ProtectYourPC>\n",
            config.oobe.protect_your_pc.as_value()
        ));

        if config.oobe.skip_machine_oobe {
            xml.push_str("                <SkipMachineOOBE>true</SkipMachineOOBE>\n");
        }
        if config.oobe.skip_user_oobe {
            xml.push_str("                <SkipUserOOBE>true</SkipUserOOBE>\n");
        }

        xml.push_str("            </OOBE>\n");

        // User accounts
        if !config.users.is_empty() || config.administrator_password.is_some() {
            xml.push_str("            <UserAccounts>\n");
            if let Some(ref administrator_password) = config.administrator_password {
                xml.push_str("                <AdministratorPassword>\n");
                xml.push_str(&format!(
                    "                    <Value>{}</Value>\n",
                    Self::encode_password(administrator_password)
                ));
                xml.push_str("                    <PlainText>false</PlainText>\n");
                xml.push_str("                </AdministratorPassword>\n");
            }

            if !config.users.is_empty() {
                xml.push_str("                <LocalAccounts>\n");

                for user in &config.users {
                    xml.push_str(&Self::generate_user_account(user));
                }

                xml.push_str("                </LocalAccounts>\n");
            }
            xml.push_str("            </UserAccounts>\n");
        }

        // Auto logon
        if let Some(ref auto_logon) = config.auto_logon {
            xml.push_str("            <AutoLogon>\n");
            xml.push_str("                <Enabled>true</Enabled>\n");

            if let Some(ref domain) = auto_logon.domain {
                xml.push_str(&format!("                <Domain>{}</Domain>\n", domain));
            }

            xml.push_str(&format!(
                "                <Username>{}</Username>\n",
                auto_logon.username
            ));
            xml.push_str("                <Password>\n");
            xml.push_str(&format!(
                "                    <Value>{}</Value>\n",
                Self::encode_password(&auto_logon.password)
            ));
            xml.push_str("                    <PlainText>false</PlainText>\n");
            xml.push_str("                </Password>\n");
            xml.push_str(&format!(
                "                <LogonCount>{}</LogonCount>\n",
                auto_logon.logon_count
            ));
            xml.push_str("            </AutoLogon>\n");
        }

        // First logon commands
        if !config.first_logon_commands.is_empty() {
            xml.push_str("            <FirstLogonCommands>\n");

            for cmd in &config.first_logon_commands {
                xml.push_str(r#"                <SynchronousCommand wcm:action="add">"#);
                xml.push('\n');
                xml.push_str(&format!(
                    "                    <Order>{}</Order>\n",
                    cmd.order
                ));
                xml.push_str(&format!(
                    "                    <CommandLine>{}</CommandLine>\n",
                    Self::escape_xml(&cmd.command_line)
                ));
                xml.push_str(&format!(
                    "                    <Description>{}</Description>\n",
                    Self::escape_xml(&cmd.description)
                ));
                xml.push_str(&format!(
                    "                    <RequiresUserInput>{}</RequiresUserInput>\n",
                    cmd.require_input
                ));
                xml.push_str("                </SynchronousCommand>\n");
            }

            xml.push_str("            </FirstLogonCommands>\n");
        }

        xml.push_str("        </component>\n");

        if let Some(ref wifi_profile) = config.wifi_profile {
            xml.push_str(&Self::generate_wifi_profile_component(wifi_profile));
        }

        // International settings for OOBE
        xml.push_str(r#"        <component name="Microsoft-Windows-International-Core" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#);
        xml.push('\n');
        xml.push_str(&format!(
            "            <InputLocale>{}</InputLocale>\n",
            config.input_locale
        ));
        xml.push_str(&format!(
            "            <SystemLocale>{}</SystemLocale>\n",
            config.language
        ));
        xml.push_str(&format!(
            "            <UILanguage>{}</UILanguage>\n",
            config.language
        ));
        xml.push_str(&format!(
            "            <UserLocale>{}</UserLocale>\n",
            config.language
        ));
        xml.push_str("        </component>\n");

        xml.push_str("    </settings>\n");
        xml
    }

    fn generate_wifi_profile_component(wifi: &WifiProfileConfig) -> String {
        let mut xml = String::new();
        let ssid = Self::escape_xml(&wifi.ssid);
        let ssid_hex = wifi
            .ssid
            .as_bytes()
            .iter()
            .map(|byte| format!("{:02X}", byte))
            .collect::<String>();
        let connection_mode = if wifi.auto_connect { "auto" } else { "manual" };

        xml.push_str(r#"        <component name="Microsoft-Windows-Wlansvc" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#);
        xml.push('\n');
        xml.push_str("            <Profiles>\n");
        xml.push_str(r#"                <Profile wcm:action="add">"#);
        xml.push('\n');
        xml.push_str(&format!("                    <name>{}</name>\n", ssid));
        xml.push_str("                    <SSIDConfig>\n");
        xml.push_str("                        <SSID>\n");
        xml.push_str(&format!(
            "                            <hex>{}</hex>\n",
            ssid_hex
        ));
        xml.push_str(&format!(
            "                            <name>{}</name>\n",
            ssid
        ));
        xml.push_str("                        </SSID>\n");
        xml.push_str(&format!(
            "                        <nonBroadcast>{}</nonBroadcast>\n",
            wifi.hidden_network
        ));
        xml.push_str("                    </SSIDConfig>\n");
        xml.push_str("                    <connectionType>ESS</connectionType>\n");
        xml.push_str(&format!(
            "                    <connectionMode>{}</connectionMode>\n",
            connection_mode
        ));
        xml.push_str("                    <MSM>\n");
        xml.push_str("                        <security>\n");
        xml.push_str("                            <authEncryption>\n");
        xml.push_str(&format!(
            "                                <authentication>{}</authentication>\n",
            wifi.authentication.as_str()
        ));
        xml.push_str(&format!(
            "                                <encryption>{}</encryption>\n",
            wifi.encryption.as_str()
        ));
        xml.push_str("                                <useOneX>false</useOneX>\n");
        xml.push_str("                            </authEncryption>\n");
        if wifi.authentication != WifiAuthentication::Open {
            xml.push_str("                            <sharedKey>\n");
            xml.push_str("                                <keyType>passPhrase</keyType>\n");
            xml.push_str("                                <protected>false</protected>\n");
            xml.push_str(&format!(
                "                                <keyMaterial>{}</keyMaterial>\n",
                Self::escape_xml(&wifi.password)
            ));
            xml.push_str("                            </sharedKey>\n");
        }
        xml.push_str("                        </security>\n");
        xml.push_str("                    </MSM>\n");
        xml.push_str("                </Profile>\n");
        xml.push_str("            </Profiles>\n");
        xml.push_str("        </component>\n");
        xml
    }

    fn generate_user_account(user: &UserAccountConfig) -> String {
        let mut xml = String::new();
        xml.push_str(r#"                    <LocalAccount wcm:action="add">"#);
        xml.push('\n');
        xml.push_str(&format!(
            "                        <Name>{}</Name>\n",
            user.username
        ));

        if let Some(ref display_name) = user.display_name {
            xml.push_str(&format!(
                "                        <DisplayName>{}</DisplayName>\n",
                display_name
            ));
        }

        xml.push_str(&format!(
            "                        <Group>{}</Group>\n",
            user.group.as_str()
        ));
        xml.push_str("                        <Password>\n");
        xml.push_str(&format!(
            "                            <Value>{}</Value>\n",
            Self::encode_password(&user.password)
        ));
        xml.push_str("                            <PlainText>false</PlainText>\n");
        xml.push_str("                        </Password>\n");
        xml.push_str("                    </LocalAccount>\n");
        xml
    }

    /// Encode password for unattend.xml (Base64 with "Password" suffix)
    fn encode_password(password: &str) -> String {
        // Windows expects password + "Password" encoded in Base64
        let with_suffix = format!("{}Password", password);
        let utf16: Vec<u16> = with_suffix.encode_utf16().collect();
        let bytes: Vec<u8> = utf16.iter().flat_map(|&x| x.to_le_bytes()).collect();
        BASE64.encode(&bytes)
    }

    /// Escape special XML characters
    fn escape_xml(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    /// Validate generated XML
    pub fn validate_xml(xml: &str) -> BitOSDTResult<()> {
        // Basic validation - check for balanced tags
        if !xml.contains("<unattend") || !xml.contains("</unattend>") {
            return Err(BitOSDTError::Validation(
                "Invalid unattend.xml: missing root element".to_string(),
            ));
        }

        // Check for proper XML declaration
        if !xml.starts_with("<?xml") {
            return Err(BitOSDTError::Validation(
                "Invalid unattend.xml: missing XML declaration".to_string(),
            ));
        }

        Ok(())
    }

    /// Generate a minimal unattend.xml that skips all OOBE screens
    pub fn generate_skip_oobe_only(language: &str, timezone: &str) -> BitOSDTResult<String> {
        let config = UnattendConfig {
            language: language.to_string(),
            timezone: timezone.to_string(),
            oobe: OobeConfig {
                skip_machine_oobe: true,
                skip_user_oobe: true,
                hide_eula: true,
                hide_wireless_setup: true,
                hide_local_account_screen: true,
                hide_online_account_screens: true,
                network_location: NetworkLocation::Work,
                protect_your_pc: ProtectYourPc::Recommended,
            },
            ..Default::default()
        };

        Self::generate(&config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_basic_unattend() {
        let config = UnattendConfig::default();
        let xml = UnattendGenerator::generate(&config).unwrap();

        assert!(xml.contains("<?xml"));
        assert!(xml.contains("<unattend"));
        assert!(xml.contains("</unattend>"));
        assert!(xml.contains("windowsPE"));
        assert!(xml.contains("specialize"));
        assert!(xml.contains("oobeSystem"));
    }

    #[test]
    fn test_generate_with_user() {
        let config = UnattendConfig {
            users: vec![UserAccountConfig {
                username: "Admin".to_string(),
                password: "P@ssw0rd".to_string(),
                display_name: Some("Administrator".to_string()),
                group: UserGroup::Administrators,
                password_never_expires: false,
                require_password_change: false,
            }],
            ..Default::default()
        };

        let xml = UnattendGenerator::generate(&config).unwrap();
        assert!(xml.contains("Admin"));
        assert!(xml.contains("Administrators"));
    }

    #[test]
    fn test_generate_with_administrator_password_and_autologon() {
        let config = UnattendConfig {
            administrator_password: Some("BootstrapPass123!".to_string()),
            auto_logon: Some(AutoLogonConfig {
                username: "Administrator".to_string(),
                password: "BootstrapPass123!".to_string(),
                domain: Some(".".to_string()),
                logon_count: 4,
            }),
            ..Default::default()
        };

        let xml = UnattendGenerator::generate(&config).unwrap();
        assert!(xml.contains("<AdministratorPassword>"));
        assert!(xml.contains("<Username>Administrator</Username>"));
        assert!(xml.contains("<Domain>.</Domain>"));
        assert!(xml.contains("<LogonCount>4</LogonCount>"));
    }

    #[test]
    fn test_generate_with_domain_join() {
        let config = UnattendConfig {
            domain_join: Some(DomainJoinConfig {
                domain: "corp.contoso.com".to_string(),
                username: "CORP\\admin".to_string(),
                password: "secret".to_string(),
                ou_path: None,
                machine_object_ou: Some("OU=Computers,DC=corp,DC=contoso,DC=com".to_string()),
            }),
            ..Default::default()
        };

        let xml = UnattendGenerator::generate(&config).unwrap();
        assert!(xml.contains("corp.contoso.com"));
        assert!(xml.contains("Microsoft-Windows-UnattendedJoin"));
    }

    #[test]
    fn test_generate_with_computer_name() {
        let config = UnattendConfig {
            computer_name: Some("ENG-WS-01".to_string()),
            ..Default::default()
        };

        let xml = UnattendGenerator::generate(&config).unwrap();
        assert!(xml.contains("<ComputerName>ENG-WS-01</ComputerName>"));
    }

    #[test]
    fn test_generate_with_wifi_profile() {
        let config = UnattendConfig {
            wifi_profile: Some(WifiProfileConfig {
                ssid: "CorpWiFi".to_string(),
                password: "WirelessP@ss123".to_string(),
                authentication: WifiAuthentication::Wpa2Psk,
                encryption: WifiEncryption::Aes,
                auto_connect: true,
                hidden_network: false,
            }),
            ..Default::default()
        };

        let xml = UnattendGenerator::generate(&config).unwrap();
        assert!(xml.contains("Microsoft-Windows-Wlansvc"));
        assert!(xml.contains("<name>CorpWiFi</name>"));
        assert!(xml.contains("<authentication>WPA2PSK</authentication>"));
        assert!(xml.contains("<keyMaterial>WirelessP@ss123</keyMaterial>"));
    }

    #[test]
    fn test_password_encoding() {
        let encoded = UnattendGenerator::encode_password("test");
        // Should be base64 encoded
        assert!(!encoded.is_empty());
        assert!(!encoded.contains("test")); // Plain text should not appear
    }

    #[test]
    fn test_xml_escaping() {
        let escaped = UnattendGenerator::escape_xml("cmd /c \"test & run\"");
        assert!(escaped.contains("&amp;"));
        assert!(escaped.contains("&quot;"));
    }
}
