use crate::page::Page;
use anyhow::{bail, Result};
use pinwheel::prelude::*;
use std::{str, str::FromStr, sync::Arc};
use tangram_app_common::{
	alerts::{AlertCadence, AlertHeuristics, AlertMetric, AlertThreshold},
	error::{bad_request, not_found, redirect_to_login, service_unavailable},
	path_components,
	user::{authorize_user, authorize_user_for_model, authorize_user_for_repo},
	Context,
};
use tangram_app_layouts::model_layout::{model_layout_info, ModelNavItem};
use tangram_id::Id;

#[derive(serde::Deserialize)]
struct Action {
	cadence: String,
	metric: String,
	threshold: String,
}

pub async fn post(request: &mut http::Request<hyper::Body>) -> Result<http::Response<hyper::Body>> {
	let context = request.extensions().get::<Arc<Context>>().unwrap().clone();
	let (repo_id, model_id) = if let ["repos", repo_id, "models", model_id, "production_alerts", "new"] =
		*path_components(request).as_slice()
	{
		(repo_id.to_owned(), model_id.to_owned())
	} else {
		bail!("unexpected path");
	};
	let mut db = match context.database_pool.begin().await {
		Ok(db) => db,
		Err(_) => return Ok(service_unavailable()),
	};
	let user = match authorize_user(request, &mut db, context.options.auth_enabled()).await? {
		Ok(user) => user,
		Err(_) => return Ok(redirect_to_login()),
	};
	let repo_id: Id = match repo_id.parse() {
		Ok(repo_id) => repo_id,
		Err(_) => return Ok(not_found()),
	};
	if !authorize_user_for_repo(&mut db, &user, repo_id).await? {
		return Ok(not_found());
	}
	let model_id: Id = match model_id.parse() {
		Ok(model_id) => model_id,
		Err(_) => return Ok(bad_request()),
	};
	if !authorize_user_for_model(&mut db, &user, model_id).await? {
		return Ok(not_found());
	}
	let data = match hyper::body::to_bytes(request.body_mut()).await {
		Ok(data) => data,
		Err(_) => return Ok(bad_request()),
	};
	let action: Action = match serde_urlencoded::from_bytes(&data) {
		Ok(action) => action,
		Err(_) => return Ok(bad_request()),
	};
	let model_layout_info =
		model_layout_info(&mut db, &context, model_id, ModelNavItem::ProductionAlerts).await?;
	let Action {
		cadence,
		metric,
		threshold,
	} = action;
	// TODO - maybe impl From<Action> for AlertHeuristics ?
	let alert = AlertHeuristics {
		cadence: AlertCadence::from_str(&cadence)?,
		threshold: AlertThreshold {
			metric: AlertMetric::from_str(&metric)?,
			variance: threshold.parse()?,
		},
	};
	let alert_json = serde_json::to_string(&alert)?;
	let result = sqlx::query(
		"
			insert into alert_preferences
				(id, alert, model_id, last_updated)
			values
				($1, $2, $3, $4)
		",
	)
	.bind(Id::generate().to_string())
	.bind(alert_json)
	.bind(model_id.to_string())
	.bind(time::OffsetDateTime::now_utc().unix_timestamp().to_string())
	.execute(&mut db)
	.await;
	if result.is_err() {
		let page = Page {
			model_layout_info,
			error: Some("There was an error creating your alert.".to_owned()),
		};
		let html = html(page);
		let response = http::Response::builder()
			.status(http::StatusCode::BAD_REQUEST)
			.body(hyper::Body::from(html))
			.unwrap();
		return Ok(response);
	};
	db.commit().await?;
	let response = http::Response::builder()
		.status(http::StatusCode::SEE_OTHER)
		.header(
			http::header::LOCATION,
			format!("/repos/{}/models/{}/production_alerts/", repo_id, model_id),
		)
		.body(hyper::Body::empty())
		.unwrap();
	Ok(response)
}