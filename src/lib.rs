use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use http_req::{
    request::{Method, Request},
    uri::Uri,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use webhook_flows::{
    create_endpoint, request_handler,
    route::{get, route, RouteError, Router},
    send_response,
};

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn on_deploy() {
    create_endpoint().await;
}

#[request_handler(get, post)]
async fn handler(
    _headers: Vec<(String, String)>,
    _subpath: String,
    _qry: HashMap<String, Value>,
    _body: Vec<u8>,
) {
    dotenv().ok();
    logger::init();

    let mut router = Router::new();
    router
        .insert("/register", vec![get(register_user)])
        .unwrap();

    if let Err(e) = route(router).await {
        match e {
            RouteError::NotFound => {
                send_response(404, vec![], b"No route matched".to_vec());
            }
            RouteError::MethodNotAllowed => {
                send_response(405, vec![], b"Method not allowed".to_vec());
            }
        }
    }
}

async fn register_user(
    _headers: Vec<(String, String)>,
    _qry: HashMap<String, Value>,
    _body: Vec<u8>,
) {
    let mut code = String::new();

    match _qry.get("code") {
        Some(m) => code = m.as_str().unwrap_or_default().to_owned(),
        _ => log::error!("missing code"),
    }

    log::error!("Code: {:?}", code);
    let token = exchange_token_w_output(code)
        .await
        .expect("failed to get token");
    let user = get_user_profile_with_his_token(&token)
        .await
        .expect("failed to get user profile");

    log::error!("User: {:?}", user);
}

async fn exchange_token_w_output(code: String) -> anyhow::Result<String> {
    let client_id = env::var("client_id").expect("github_client_id is required");
    let client_secret = env::var("client_secret").expect("github_client_secret is required");
    let url = format!("https://github.com/login/oauth/access_token");
    // let url = format!("https://github.com/login/oauth/access_token?client_id={client_id}&client_secret={client_secret}&code={code}&redirect_uri=https://code.flows.network/webhook/jKRuADFii4naC7ANMFtL/register");

    let params = json!({
        "client_id": client_id,
        "client_secret": client_secret,
        "code": code,
        "grant_type": "authorization_code",
    })
    .to_string();
    // "redirect_uri": "https://code.flows.network/webhook/jKRuADFii4naC7ANMFtL/register"

    let writer = github_http_post(&url, &params).await?;
    // "access_token=blurred&scope=read%3Auser&token_type=bearer\"",
    let stuff_in_writer = String::from_utf8_lossy(&writer);
    log::error!("Exchange token Response: {:?}", stuff_in_writer);

    let load: Load = serde_json::from_slice(&writer)?;

    #[derive(Debug, Deserialize, Serialize, Clone, Default)]
    struct Load {
        access_token: Option<String>,
        scope: Option<String>,
        token_type: Option<String>,
    }

    match load.access_token {
        Some(m) => Ok(m),
        None => {
            log::error!("failed to get token");
            anyhow::bail!("failed to get token")
        }
    }
}

pub async fn github_http_post(url: &str, query: &str) -> anyhow::Result<Vec<u8>> {
    let token = env::var("GITHUB_TOKEN").expect("github_token is required");
    let mut writer = Vec::new();

    let uri = Uri::try_from(url).expect("failed to parse url");

    match Request::new(&uri)
        .method(Method::POST)
        .header("User-Agent", "flows-network connector")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Authorization", &format!("Bearer {}", token))
        .header("Content-Length", &query.to_string().len())
        .body(&query.to_string().into_bytes())
        .send(&mut writer)
    {
        Ok(res) => {
            if !res.status_code().is_success() {
                log::error!("Github http error {:?}", res.status_code());
                return Err(anyhow::anyhow!("Github http error {:?}", res.status_code()));
            }
            Ok(writer)
        }
        Err(_e) => {
            log::error!("Error getting response from Github: {:?}", _e);
            Err(anyhow::anyhow!(_e))
        }
    }
}
pub async fn github_http_get(url: &str, token: &str) -> anyhow::Result<Vec<u8>> {
    let mut writer = Vec::new();
    let url = Uri::try_from(url).unwrap();

    match Request::new(&url)
        .method(Method::GET)
        .header("User-Agent", "flows-network connector")
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", token))
        .header("CONNECTION", "close")
        .send(&mut writer)
    {
        Ok(res) => {
            if !res.status_code().is_success() {
                log::error!("Github http error {:?}", res.status_code());
                return Err(anyhow::anyhow!("Github http error {:?}", res.status_code()));
            }
            Ok(writer)
        }
        Err(_e) => {
            log::error!("Error getting response from Github: {:?}", _e);
            Err(anyhow::anyhow!(_e))
        }
    }
}

pub async fn get_user_profile_with_his_token(
    token: &str,
) -> anyhow::Result<(String, String, String, String)> {
    #[derive(Debug, Deserialize, Serialize, Clone, Default)]
    struct User {
        name: Option<String>,
        login: Option<String>,
        twitter_username: Option<String>,
        email: Option<String>,
    }

    let base_url = "https://api.github.com/user";

    let writer = github_http_get(&base_url, &token).await?;

    let user: User = serde_json::from_slice(&writer)?;
    let name = user.name.unwrap_or_default();
    let login = user.login.unwrap_or_default();
    let twitter_username = user.twitter_username.unwrap_or_default();
    let email = user.email.unwrap_or_default();
    Ok((name, login, twitter_username, email))
}
