use std::{net::SocketAddr, time::Duration};

use dotenvy::dotenv;
use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::bank_web::BankWeb;

mod bank;
mod bank_web;
mod errors;

pub async fn pg_pool() -> Result<PgPool, sqlx::Error> {
    dotenv().expect("failed to load .env");

    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(1))
        .connect(&std::env::var("DATABASE_URL").expect("DATABASE_URL must be in environment"))
        .await
}

#[tokio::main]
async fn main() {
    dotenv().expect("failed to load .env");

    init_tracing();

    let pool = pg_pool().await.expect("failed to connect to postgres");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("failed to run sqlx migrations");

    let account_service = bank::accounts::DummyService::default();
    let router = BankWeb::new(pool, account_service).into_router();

    let addr = SocketAddr::from(([127, 0, 0, 1], 4000));
    tracing::info!("listening on http://{}", addr);

    axum::Server::bind(&addr)
        .serve(router.into_make_service())
        .await
        .expect("failed to serve");
}

pub fn init_tracing() {
    use opentelemetry_otlp::WithExportConfig;
    use tracing_subscriber::prelude::*;

    let filter_layer = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let fmt_layer =
        tracing_subscriber::fmt::layer().event_format(tracing_subscriber::fmt::format().pretty());

    let otel_exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint("http://localhost:4317");

    let otel_resource = opentelemetry::sdk::Resource::new([opentelemetry::KeyValue::new(
        "service.name",
        "hiring_challenge_rust",
    )]);

    let otel_tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(otel_exporter)
        .with_trace_config(
            opentelemetry::sdk::trace::config()
                .with_resource(otel_resource)
                .with_sampler(opentelemetry::sdk::trace::Sampler::AlwaysOn),
        )
        .install_simple()
        .unwrap();

    let otel_trace_layer = tracing_opentelemetry::layer().with_tracer(otel_tracer);

    tracing_subscriber::Registry::default()
        .with(filter_layer)
        .with(fmt_layer)
        .with(otel_trace_layer)
        .init();
}
