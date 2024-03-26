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
    route::{get, post, route, RouteError, Router},
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
        .insert("/register", vec![get(register_user), post(parse_token)]).unwrap();

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
    log::error!("Token obtained in Register: {:?}", token);
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
    let parsed: Vec<(String, String)> = form_urlencoded::parse(writer.as_slice())
        .into_owned()
        .collect();
    let temp_str = String::new();
    let mut token = parsed
        .iter()
        .find(|(k, _)| k == "access_token")
        .map(|(_, v)| v.to_string())
        .unwrap_or(temp_str);

    log::error!("Token parsed from params: {:?}", token.clone());

    if token.is_empty() {
        let load: Load = match serde_json::from_slice(&writer) {
            Ok(m) => {
                log::error!("{m:?}");
                m
            }

            Err(e) => {
                log::error!("failed to parse access token response: {:?}", e);
                panic!()
            }
        };
        #[derive(Debug, Deserialize, Serialize, Clone, Default)]
        struct Load {
            access_token: Option<String>,
            scope: Option<String>,
            token_type: Option<String>,
        }
        log::error!("Token parsed from bdoy: {:?}", token.clone());

        match load.access_token {
            Some(m) => token = m,
            None => {
                log::error!("failed to get token");
                panic!()
            }
        }
    }

    Ok(token.to_string())
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

/* async fn exchange_token(code: String) -> anyhow::Result<()> {
    let client_id = env::var("client_id").expect("github_client_id is required");
    let client_secret = env::var("client_secret").expect("github_client_secret is required");
    let url = format!("https://github.com/login/oauth/access_token");
    // let url = format!("https://github.com/login/oauth/access_token?client_id={client_id}&client_secret={client_secret}&code={code}&redirect_uri=https://code.flows.network/webhook/jKRuADFii4naC7ANMFtL/register");

    let params = json!({
        "client_id": client_id,
        "client_secret": client_secret,
        "code": code,
        "redirect_uri": "https://code.flows.network/webhook/jKRuADFii4naC7ANMFtL/register"
    })
    .to_string();

    log::error!("URL: {:?}", url);
    let _ = github_http_post(&url, &params).await?;
    Ok(())
} */

async fn parse_token(
    _headers: Vec<(String, String)>,
    _qry: HashMap<String, Value>,
    _body: Vec<u8>,
) {
    log::error!("post query: {:?}", _qry);
    // let mut token = String::new();
    // match _qry.get("access_token") {
    //     Some(m) => token = m.as_str().unwrap_or_default().to_owned(),
    //     _ => log::error!("missing code"),
    // };

    #[derive(Debug, Deserialize, Serialize, Clone, Default)]
    struct Load {
        access_token: Option<String>,
        scope: Option<String>,
        token_type: Option<String>,
    }

    let load: Load = serde_json::from_slice(&_body).unwrap();

    let token = load.access_token.unwrap_or_default();

    log::error!("Token: {:?}", token);
    let user = get_user_profile_with_his_token(&token)
        .await
        .expect("failed to get user profile");

    log::error!("User: {:?}", user);
}
