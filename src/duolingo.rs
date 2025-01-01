use anyhow::{anyhow, bail, Context, Result};
use chrono::{Local, NaiveTime};
use log::{debug, error};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Minimal wrapper of the duolingo api.
/// Requires copying a token from a properly logged-in browser instance.
#[derive(Clone)]
pub struct DuolingoApi {
    client: Client,
    jwt: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JWTClaims {
    sub: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyProgress {
    pub xp_goal: u32,
    pub xp_today: u32,
    pub lessons_today: Vec<Lesson>,
}

/// A single lesson entry
#[derive(Debug, Serialize, Deserialize)]
pub struct Lesson {
    /// Seconds since unix epoch
    pub time: i64,
    pub xp: u32,
}

impl DuolingoApi {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) Chrome/83.0.4103.116 DuolingoEnforcer/1.0")
            .build()
            .context("Failed to build request client")?;

        Ok(Self {
            client,
            jwt: None,
            user_id: None,
        })
    }

    /// If we used an empty string for JWT, or want to change it later
    pub async fn update_jwt(&mut self, new_jwt: &str) -> Result<()> {
        self.jwt = Some(new_jwt.to_string());

        use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

        // For ignoring the signature (like Python code verify=False):
        let mut validation = Validation::new(Algorithm::HS256);
        validation.insecure_disable_signature_validation();

        let token_data = decode::<JWTClaims>(
            new_jwt,
            &DecodingKey::from_secret(b"ignore_signature"),
            &validation,
        )
        .map_err(|e| anyhow::anyhow!("Failed to decode JWT's sub: {e}"))?;
        let user_id = token_data.claims.sub.to_string();

        self.check_auth(&user_id).await?;
        self.user_id = Some(user_id);
        Ok(())
    }

    async fn check_auth(&mut self, user_id: &str) -> Result<()> {
        let jwt = self.jwt.as_ref().ok_or_else(|| anyhow!("Missing jwt"))?;
        let url = format!(
            "https://www.duolingo.com/2017-06-30/users/{}?fields=username",
            user_id
        );
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "DuoEnforcer/0.1")
            .bearer_auth(jwt)
            .send()
            .await?;
        if resp.status() != 200 {
            bail!("Failed to fetch username (status={})", resp.status());
        }

        #[derive(Debug, Deserialize)]
        struct RespData {
            username: String,
        }
        let val: RespData = resp.json().await?;
        debug!("Got username {}", val.username);

        Ok(())
    }

    pub async fn get_daily_progress(&self) -> Result<DailyProgress> {
        let jwt = self.jwt.as_ref().ok_or_else(|| anyhow!("Missing jwt"))?;
        let user_id = self
            .user_id
            .as_ref()
            .ok_or_else(|| anyhow!("Missing user_id"))?;

        let url = format!(
            "https://www.duolingo.com/2017-06-30/users/{user_id}?fields=xpGoal,xpGains,streakData",
        );
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "DuoEnforcer/0.1")
            .bearer_auth(jwt)
            .send()
            .await?;

        if resp.status() != 200 {
            bail!(
                "daily xp fetch returned status={}, {}",
                resp.status(),
                resp.text()
                    .await
                    .context("Failed to read text in http response")?
            );
        }
        #[derive(Debug, Deserialize)]
        struct RespData {
            #[serde(rename = "xpGoal")]
            xp_goal: u32,
            #[serde(rename = "xpGains")]
            xp_gains: Vec<Lesson>,
            #[serde(rename = "streakData")]
            #[allow(dead_code)]
            streak_data: StreakData,
        }
        #[derive(Debug, Deserialize)]
        struct StreakData {
            #[serde(rename = "updatedTimestamp")]
            #[allow(dead_code)]
            updated_timestamp: i64,
        }

        let daily: RespData = resp.json().await?;
        debug!("Read xp data: {daily:?}");

        // local midnight expressed as unix time
        let midnight = match Local::now().with_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()) {
            chrono::offset::LocalResult::Single(t) => t.timestamp(),
            chrono::offset::LocalResult::Ambiguous(earliest, _) => earliest.timestamp(),
            chrono::offset::LocalResult::None => {
                error!("No midnight today. Looking in past 24 hours instead");
                Local::now().timestamp() - 60 * 60 * 24
            }
        };
        debug!("Using last midnight timestamp: {midnight}");

        let lessons_today: Vec<Lesson> = daily
            .xp_gains
            .into_iter()
            .filter(|l| l.time > midnight)
            .collect();
        let xp_today = lessons_today.iter().map(|l| l.xp).sum();

        Ok(DailyProgress {
            xp_goal: daily.xp_goal,
            lessons_today,
            xp_today,
        })
    }
}
