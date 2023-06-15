use crate::{setting::SettingWrapper, App, Extension};
use actix_web::{web, HttpResponse};
use metrics::describe_counter;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde::Deserialize;

#[derive(Deserialize, Default, Debug)]
pub struct MetricsSetting {
    pub enabled: bool,
    pub auth: Option<String>,
}

pub struct Metrics {
    pub handle: web::Data<PrometheusHandle>,
}

impl Metrics {
    pub fn new() -> Self {
        describe_metrics();
        Self {
            handle: web::Data::new(create_prometheus_handle()),
        }
    }
}

impl Extension for Metrics {
    fn name(&self) -> &'static str {
        "metrics"
    }

    fn setting(&mut self, setting: &SettingWrapper) {
        setting.write().load_extension::<MetricsSetting>("metrics");
    }

    fn config_web(&mut self, cfg: &mut actix_web::web::ServiceConfig) {
        cfg.app_data(self.handle.clone())
            .service(web::resource("/metrics").route(web::get().to(route_metrics)));
    }
}

pub fn describe_metrics() {
    describe_counter!("new_connections", "The total count of new connections");
    describe_counter!("current_connections", "The number of current connections");
}

pub fn create_prometheus_handle() -> PrometheusHandle {
    let builder = PrometheusBuilder::new();
    builder
        // .idle_timeout(
        //     metrics_util::MetricKindMask::ALL,
        //     Some(std::time::Duration::from_secs(10)),
        // )
        .install_recorder()
        .unwrap()
}

#[derive(Deserialize, Default)]
struct Info {
    auth: Option<String>,
}

async fn route_metrics(
    handle: web::Data<PrometheusHandle>,
    app: web::Data<App>,
    query: web::Query<Info>,
) -> Result<HttpResponse, actix_web::Error> {
    let setting = app.setting.read();
    if let Some(s) = setting.get_extension::<MetricsSetting>() {
        if s.enabled && s.auth == query.auth {
            return Ok(HttpResponse::Ok()
                .insert_header(("Content-Type", "text/plain"))
                .body(handle.render()));
        }
    }
    Ok(HttpResponse::NotFound().finish())
}

#[cfg(test)]
pub mod tests {
    use super::Metrics;
    use crate::create_test_app;
    use actix_rt::time::sleep;
    use actix_web::{
        dev::Service,
        test::{init_service, read_body, TestRequest},
    };
    use anyhow::Result;
    use std::time::Duration;

    #[actix_rt::test]
    async fn metrics() -> Result<()> {
        let data = create_test_app("")?;
        {
            let mut w = data.setting.write();
            w.extra = serde_json::from_str(
                r#"{
                "metrics": {
                    "enabled": true,
                    "auth": "auth_key"
                }
            }"#,
            )?;
        }
        let data = data.add_extension(Metrics::new());

        let app = init_service(data.web_app()).await;
        sleep(Duration::from_millis(50)).await;
        metrics::increment_counter!("test_metric");

        let req = TestRequest::with_uri("/metrics").to_request();
        let res = app.call(req).await.unwrap();
        assert_eq!(res.status(), 404);

        let req = TestRequest::with_uri("/metrics?auth=auth_key").to_request();
        let res = app.call(req).await.unwrap();
        assert_eq!(res.status(), 200);
        let result = read_body(res).await;
        let result = String::from_utf8(result.to_vec())?;
        assert!(result.contains("test_metric"));
        Ok(())
    }
}
