//! The ChatGPT sign-in parts that touch neither the network nor the disk: the PKCE pair, the
//! authorization link, the browser's callback, and the checks on the ID token.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use url::Url;

pub const AUTHORIZE_URL: &str = "https://auth.openai.com/api/accounts/authorize";
pub const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
pub const REVOKE_URL: &str = "https://auth.openai.com/api/accounts/oauth/revoke";
pub const JWKS_URL: &str = "https://auth.openai.com/.well-known/jwks.json";
pub const ISSUER: &str = "https://auth.openai.com";
/// The resource the plan's access token is for, in the link and in the token exchange.
pub const RESOURCE: &str = "https://api.openai.com/v1";
/// The placeholder client of a first login. OpenAI answers with the client it issued.
pub const REGISTRATION_CLIENT: &str = "dynamic_agent_client";
/// The name the consent screen shows on a first login.
pub const AGENT_NAME: &str = "Lynx";
pub const SCOPE: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
/// The scope that lets Lynx spend the user's plan. An ID token alone does not grant it.
pub const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
pub const CALLBACK_PATH: &str = "/auth/callback";

#[derive(Debug, thiserror::Error)]
pub enum ChatGptError {
    #[error("the sign-in was refused in the browser")]
    Refused,
    #[error("the sign-in did not finish in time")]
    TimedOut,
    #[error("the browser's answer does not belong to this sign-in")]
    StateMismatch,
    #[error("OpenAI did not issue a client ID for Lynx")]
    RegistrationIncomplete,
    #[error("OpenAI answered for another account than the one signed in")]
    AccountChanged,
    #[error("ChatGPT plan use is not enabled for this account")]
    PlanNotEnabled,
    #[error("OpenAI's ID token did not check out: {0}")]
    InvalidIdToken(&'static str),
    #[error("could not reach OpenAI: {0}")]
    Network(String),
    #[error("could not finish the sign-in on this computer: {0}")]
    Local(String),
    /// OpenAI answered with a refusal. The full answer is in the log; this keeps only its code.
    #[error("OpenAI answered {status}: {}", .code.as_deref().unwrap_or("no error code"))]
    Answer { status: u16, code: Option<String> },
}

/// The `error` code of an OpenAI refusal, such as `invalid_grant`. None when there is none.
pub fn error_code(answer: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(answer).ok()?;
    value.get("error")?.as_str().map(str::to_owned)
}

/// The PKCE pair of one attempt. The verifier stays in Lynx; the challenge goes in the link.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

/// `random` is 32 bytes from the operating system, so the verifier carries the entropy.
pub fn pkce(random: &[u8]) -> Pkce {
    let verifier = URL_SAFE_NO_PAD.encode(random);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Pkce {
        verifier,
        challenge,
    }
}

/// What one attempt's authorization link carries.
pub struct Authorization<'a> {
    pub client_id: &'a str,
    /// Only a first login names the app. A later one already has its client.
    pub agent_name_hint: Option<&'a str>,
    /// This install's identity. It is the same on every sign-in from the same install.
    pub host_id: &'a str,
    pub redirect_uri: &'a str,
    pub state: &'a str,
    pub nonce: &'a str,
    pub challenge: &'a str,
    /// The ID token of the last sign-in, so OpenAI skips the account picker.
    pub id_token_hint: Option<&'a str>,
}

pub fn authorize_url(request: &Authorization<'_>) -> Result<Url, url::ParseError> {
    let mut url = Url::parse(AUTHORIZE_URL)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("client_id", request.client_id);
        if let Some(name) = request.agent_name_hint {
            query.append_pair("agent_name_hint", name);
        }
        query
            .append_pair("ext_agent_host_id", request.host_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", request.redirect_uri)
            .append_pair("scope", SCOPE)
            .append_pair("resource", RESOURCE)
            .append_pair("state", request.state)
            .append_pair("nonce", request.nonce)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", request.challenge);
        if let Some(hint) = request.id_token_hint {
            query.append_pair("id_token_hint", hint);
        }
    }
    // Form encoding writes a space as '+', but the link is URL-encoded, where a space is %20.
    let query = url.query().map(|query| query.replace('+', "%20"));
    url.set_query(query.as_deref());
    Ok(url)
}

/// What the browser sent back to the loopback listener.
#[derive(Debug, PartialEq, Eq)]
pub enum Callback {
    Code {
        code: String,
        state: String,
        /// Sent when OpenAI issues a client, and again on a later sign-in.
        client_id: Option<String>,
    },
    Refused,
}

/// Reads the request target of a browser visit (`/auth/callback?code=…`). None for any other path.
pub fn parse_callback(target: &str) -> Option<Callback> {
    // Query strings are form-encoded, where a raw '+' reads as a space. In a code it is a plus.
    let target = target.replace('+', "%2B");
    let url = Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != CALLBACK_PATH {
        return None;
    }
    let query: HashMap<String, String> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if query.contains_key("error") {
        return Some(Callback::Refused);
    }
    Some(Callback::Code {
        code: query.get("code")?.clone(),
        state: query.get("state")?.clone(),
        client_id: query.get("client_id").cloned(),
    })
}

/// What the token endpoint returns. Only the fields Lynx keeps.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub expires_in: i64,
    #[serde(default)]
    pub scope: String,
}

/// Trades the code for tokens. The verifier proves this attempt is the one that asked for it.
pub fn code_exchange_form<'a>(
    code: &'a str,
    verifier: &'a str,
    client_id: &'a str,
    redirect_uri: &'a str,
) -> [(&'static str, &'a str); 6] {
    [
        ("grant_type", "authorization_code"),
        ("client_id", client_id),
        ("code", code),
        ("code_verifier", verifier),
        ("redirect_uri", redirect_uri),
        ("resource", RESOURCE),
    ]
}

/// Ends a session at OpenAI. The refresh token is the one it revokes.
pub fn revoke_form<'a>(refresh_token: &'a str, client_id: &'a str) -> [(&'static str, &'a str); 3] {
    [
        ("token", refresh_token),
        ("token_type_hint", "refresh_token"),
        ("client_id", client_id),
    ]
}

/// Whether a granted scope list lets Lynx use the plan.
pub fn grants_plan_use(scope: &str) -> bool {
    scope
        .split_whitespace()
        .any(|granted| granted == PLAN_SCOPE)
}

/// The keys OpenAI signs ID tokens with, as published at `JWKS_URL`.
#[derive(Debug, Deserialize)]
pub struct Jwks {
    pub keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
pub struct Jwk {
    pub kty: String,
    pub kid: Option<String>,
    pub n: Option<String>,
    pub e: Option<String>,
}

/// Who signed in, from an ID token that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub subject: String,
    pub email: Option<String>,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: Audience,
    sub: String,
    exp: i64,
    nonce: Option<String>,
    email: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

impl Audience {
    fn includes(&self, client_id: &str) -> bool {
        match self {
            Self::One(audience) => audience == client_id,
            Self::Many(audiences) => audiences.iter().any(|audience| audience == client_id),
        }
    }
}

/// Checks the signature against the published keys, then the claims. Returns who signed in.
pub fn verify_id_token(
    token: &str,
    keys: &Jwks,
    client_id: &str,
    nonce: &str,
    now: i64,
) -> Result<Identity, ChatGptError> {
    let (signing_input, signature) = token
        .rsplit_once('.')
        .ok_or(ChatGptError::InvalidIdToken("not a JWT"))?;
    let (header_part, payload_part) = signing_input
        .split_once('.')
        .ok_or(ChatGptError::InvalidIdToken("not a JWT"))?;
    if payload_part.contains('.') {
        return Err(ChatGptError::InvalidIdToken("not a JWT"));
    }

    let header: Header = decode_part(header_part)?;
    if header.alg != "RS256" {
        return Err(ChatGptError::InvalidIdToken("the signature is not RS256"));
    }
    let kid = header
        .kid
        .ok_or(ChatGptError::InvalidIdToken("no key ID"))?;
    let key = keys
        .keys
        .iter()
        .find(|key| key.kty == "RSA" && key.kid.as_deref() == Some(kid.as_str()))
        .ok_or(ChatGptError::InvalidIdToken("no published key matches"))?;
    let (Some(n), Some(e)) = (&key.n, &key.e) else {
        return Err(ChatGptError::InvalidIdToken("the key has no modulus"));
    };
    let n = URL_SAFE_NO_PAD
        .decode(n)
        .map_err(|_| ChatGptError::InvalidIdToken("the key is not base64url"))?;
    let e = URL_SAFE_NO_PAD
        .decode(e)
        .map_err(|_| ChatGptError::InvalidIdToken("the key is not base64url"))?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| ChatGptError::InvalidIdToken("the signature is not base64url"))?;
    RsaPublicKeyComponents {
        n: n.as_slice(),
        e: e.as_slice(),
    }
    .verify(
        &RSA_PKCS1_2048_8192_SHA256,
        signing_input.as_bytes(),
        &signature,
    )
    .map_err(|_| ChatGptError::InvalidIdToken("the signature does not match"))?;

    let claims: Claims = decode_part(payload_part)?;
    check_claims(&claims, client_id, nonce, now)?;
    Ok(Identity {
        subject: claims.sub,
        email: claims.email,
    })
}

/// The issuer, the client this attempt registered, the expiry, and the nonce this attempt sent.
fn check_claims(
    claims: &Claims,
    client_id: &str,
    nonce: &str,
    now: i64,
) -> Result<(), ChatGptError> {
    if claims.iss != ISSUER {
        return Err(ChatGptError::InvalidIdToken("the issuer is not OpenAI"));
    }
    if !claims.aud.includes(client_id) {
        return Err(ChatGptError::InvalidIdToken(
            "the token is for another client",
        ));
    }
    if claims.exp <= now {
        return Err(ChatGptError::InvalidIdToken("the token has expired"));
    }
    if claims.nonce.as_deref() != Some(nonce) {
        return Err(ChatGptError::InvalidIdToken("the nonce does not match"));
    }
    if claims.sub.is_empty() {
        return Err(ChatGptError::InvalidIdToken("no subject"));
    }
    Ok(())
}

fn decode_part<T: DeserializeOwned>(part: &str) -> Result<T, ChatGptError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(part)
        .map_err(|_| ChatGptError::InvalidIdToken("not base64url"))?;
    serde_json::from_slice(&bytes).map_err(|_| ChatGptError::InvalidIdToken("not JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Signed with a throwaway key whose private half is not kept. Only its public half is here.
    const JWKS: &str = r#"{"keys":[{"kty":"RSA","kid":"test-key","alg":"RS256","use":"sig","n":"5Ow1kDuqT4OONbLoSfIlRf6TH8P02YEP4AYx_2SrrSLYjKnbP9u6tz-oRLSFl99wrJVprbKhM3uOUKeLyw4Etw6IQiiyjESn96i837SkPvvpzLzIydqN2086inoFaUm-lSxoa17Jt_cJ7hklTXjs8W3N3gySX_gyCGDNYIL1I35VuU8gpY7fQTV8tHtaTSOIYJ_eS8ozYov9XOQKOpPtJlRJtcbzKf2ADbycmW9beZZZp0DgIMFQBV0l66SwGAkJqBve1tWTgGInmxmtf06I7YxSAa0x6DCWg3Qod3BfdQN-VTGj9RqEPlKNppW-gP7Vf9lERDLeMeHnlBhyyFaYsw","e":"AQAB"}]}"#;
    const TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InRlc3Qta2V5IiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2F1dGgub3BlbmFpLmNvbSIsImF1ZCI6Im9haWFwcF90ZXN0Iiwic3ViIjoidXNlci0xIiwiZXhwIjo0MTAyNDQ0ODAwLCJub25jZSI6Im4tMSIsImVtYWlsIjoiZGV2QGV4YW1wbGUuY29tIn0.ZHkuwh6YnUZTWmCZPqMDwB6fcpTat_CHCCG8UCazgIe0VcgffZih-XprlsDHxz18-EpJnUZFefSyULKFcfsKDIp_7Blb-DBe_S7K8Lh1sB1JAJnpYCk-IbOtJCrpwQZzb9oAFVYkDsmFG_zPFYuCO4kiUTvBfYU9U8DK5seo_iwx_wKHpaOhWmXGutU3FMAPiqR1-yBMf2V1lKkUPallA_CxV5wb2KlsMTbmkFqDdhnj6aFTLDX0T25ml5iyPqTC7rqBsMBALZvYhkiMvT_U36ku0uaclGwCdI8zQB_qm4v_1bDmtom3e_PYcrrQ1Ez-wfvjqnCOOCdbfUJ9xbeczg";
    const NOW: i64 = 1_800_000_000;

    fn keys() -> Jwks {
        serde_json::from_str(JWKS).expect("keys")
    }

    fn link(client_id: &str, agent_name_hint: Option<&str>, id_token_hint: Option<&str>) -> Url {
        let request = Authorization {
            client_id,
            agent_name_hint,
            host_id: "urn:uuid:host",
            redirect_uri: "http://127.0.0.1:5000/auth/callback",
            state: "state-1",
            nonce: "n-1",
            challenge: "challenge-1",
            id_token_hint,
        };
        authorize_url(&request).expect("link")
    }

    fn query(url: &Url) -> HashMap<String, String> {
        url.query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect()
    }

    #[test]
    fn the_pkce_challenge_matches_the_rfc_7636_example() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let random = URL_SAFE_NO_PAD.decode(verifier).expect("base64url");

        let pair = pkce(&random);

        assert_eq!(pair.verifier, verifier);
        assert_eq!(
            pair.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_first_login_names_the_app_and_registers_the_placeholder() {
        let url = query(&link(REGISTRATION_CLIENT, Some(AGENT_NAME), None));

        assert_eq!(url["client_id"], REGISTRATION_CLIENT);
        assert_eq!(url["agent_name_hint"], AGENT_NAME);
        assert_eq!(url["ext_agent_host_id"], "urn:uuid:host");
        assert_eq!(url["scope"], SCOPE);
        assert!(
            link(REGISTRATION_CLIENT, Some(AGENT_NAME), None)
                .as_str()
                .contains("scope=openid%20profile%20email%20offline_access"),
            "spaces must be %20, not '+'"
        );
        assert_eq!(url["resource"], RESOURCE);
        assert_eq!(url["code_challenge_method"], "S256");
        assert_eq!(url["code_challenge"], "challenge-1");
        assert!(!url.contains_key("id_token_hint"));
    }

    #[test]
    fn a_later_login_sends_the_issued_client_and_the_last_id_token() {
        let url = query(&link("oaiapp_test", None, Some("id-token")));

        assert_eq!(url["client_id"], "oaiapp_test");
        assert!(!url.contains_key("agent_name_hint"));
        assert_eq!(url["id_token_hint"], "id-token");
    }

    #[test]
    fn the_callback_gives_the_code_or_a_refusal() {
        assert_eq!(
            parse_callback("/auth/callback?code=c1&state=s1&client_id=oaiapp_test"),
            Some(Callback::Code {
                code: "c1".to_owned(),
                state: "s1".to_owned(),
                client_id: Some("oaiapp_test".to_owned()),
            })
        );
        assert_eq!(
            parse_callback("/auth/callback?error=access_denied"),
            Some(Callback::Refused)
        );
        assert_eq!(parse_callback("/favicon.ico"), None);
        assert_eq!(parse_callback("/auth/callback?code=c1"), None);
        assert_eq!(
            parse_callback("/auth/callback?code=a+b&state=s1"),
            Some(Callback::Code {
                code: "a+b".to_owned(),
                state: "s1".to_owned(),
                client_id: None,
            })
        );
    }

    #[test]
    fn a_token_signed_with_a_published_key_passes() {
        let identity = verify_id_token(TOKEN, &keys(), "oaiapp_test", "n-1", NOW);

        assert_eq!(
            identity.expect("identity"),
            Identity {
                subject: "user-1".to_owned(),
                email: Some("dev@example.com".to_owned()),
            }
        );
    }

    #[test]
    fn a_changed_payload_fails_the_signature() {
        let mut parts: Vec<&str> = TOKEN.split('.').collect();
        parts[1] = "eyJpc3MiOiJodHRwczovL2F1dGgub3BlbmFpLmNvbSIsImF1ZCI6Im9haWFwcF90ZXN0Iiwic3ViIjoiYWRtaW4iLCJleHAiOjQxMDI0NDQ4MDAsIm5vbmNlIjoibi0xIn0";

        let tampered = parts.join(".");
        let error = verify_id_token(&tampered, &keys(), "oaiapp_test", "n-1", NOW);

        assert!(matches!(
            error,
            Err(ChatGptError::InvalidIdToken("the signature does not match"))
        ));
    }

    #[test]
    fn the_claims_must_match_this_attempt() {
        let claims =
            |value: serde_json::Value| -> Claims { serde_json::from_value(value).expect("claims") };
        let good = serde_json::json!({
            "iss": ISSUER, "aud": "oaiapp_test", "sub": "user-1", "exp": NOW + 60, "nonce": "n-1"
        });

        assert!(check_claims(&claims(good.clone()), "oaiapp_test", "n-1", NOW).is_ok());
        assert!(check_claims(&claims(good.clone()), "oaiapp_other", "n-1", NOW).is_err());
        assert!(check_claims(&claims(good.clone()), "oaiapp_test", "n-2", NOW).is_err());
        assert!(check_claims(&claims(good.clone()), "oaiapp_test", "n-1", NOW + 61).is_err());
        let mut many = good.clone();
        many["aud"] = serde_json::json!(["x", "oaiapp_test"]);
        assert!(check_claims(&claims(many), "oaiapp_test", "n-1", NOW).is_ok());
        let mut wrong_issuer = good;
        wrong_issuer["iss"] = serde_json::json!("https://example.com");
        assert!(check_claims(&claims(wrong_issuer), "oaiapp_test", "n-1", NOW).is_err());
    }

    #[test]
    fn the_refusal_code_is_read_and_missing_codes_are_none() {
        assert_eq!(
            error_code(r#"{"error":"invalid_grant","error_description":"expired"}"#),
            Some("invalid_grant".to_owned())
        );
        assert_eq!(error_code("<html>Just a moment...</html>"), None);
        assert_eq!(error_code(r#"{"message":"no code here"}"#), None);
    }

    #[test]
    fn only_the_plan_scope_lets_lynx_use_the_plan() {
        assert!(grants_plan_use(
            "openid chatgpt.tokens.use.direct offline_access"
        ));
        assert!(!grants_plan_use("openid profile email offline_access"));
    }
}
