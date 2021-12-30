//! The App presents a Rust API for interacting with Tangram.

use crate::{
	alerts::{
		check_for_duplicate_monitor, create_monitor, get_monitor, update_monitor, AlertCadence,
		AlertMethod, Monitor, MonitorThreshold,
	},
	options, storage,
};
use anyhow::{anyhow, Result};
use tangram_id::Id;

pub struct App {
	pub database_pool: sqlx::AnyPool,
	pub options: options::Options,
	pub smtp_transport: Option<lettre::AsyncSmtpTransport<lettre::Tokio1Executor>>,
	pub storage: self::storage::Storage,
}

impl App {
	pub async fn create_monitor(
		&self,
		db: &mut sqlx::Transaction<'_, sqlx::Any>,
		cadence: AlertCadence,
		methods: &[AlertMethod],
		model_id: Id,
		threshold: MonitorThreshold,
		title: &str,
	) -> Result<()> {
		let mut monitor = Monitor {
			cadence,
			id: Id::generate(),
			methods: methods.to_owned(),
			model_id,
			threshold,
			title: title.to_owned(),
		};
		if monitor.title.is_empty() {
			monitor.title = monitor.default_title();
		}

		if check_for_duplicate_monitor(db, &monitor, model_id).await? {
			return Err(anyhow!("Identical alert already exists"));
		}

		create_monitor(db, monitor, model_id).await?;

		Ok(())
	}

	pub async fn update_monitor(
		&self,
		db: &mut sqlx::Transaction<'_, sqlx::Any>,
		monitor_id: Id,
		cadence: AlertCadence,
		methods: &[AlertMethod],
		model_id: Id,
		threshold: MonitorThreshold,
		title: &str,
	) -> Result<()> {
		let mut monitor = get_monitor(db, monitor_id).await?;
		let mut title = title.to_owned();
		if title.is_empty() {
			title = monitor.default_title();
		}

		// Replace any components that are different.
		if cadence != monitor.cadence {
			monitor.cadence = cadence;
		}
		if methods != monitor.methods {
			monitor.methods = methods.to_owned();
		}
		if threshold != monitor.threshold {
			monitor.threshold = threshold;
		}
		if title != monitor.title {
			monitor.title = title;
		}

		if check_for_duplicate_monitor(db, &monitor, model_id).await? {
			return Err(anyhow!("Identical alert already exists"));
		}

		update_monitor(db, &monitor, monitor_id).await?;

		Ok(())
	}
}