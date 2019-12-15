use actix_web::{error, middleware, web, App, Error, HttpResponse, HttpServer};
use listenfd::ListenFd;
use r2d2_postgres::TlsMode;
use actix_files as fs;
use serde::{Deserialize, Serialize};
use std::io;

fn set_up_tera() -> Result<tera::Tera, tera::Error> {
    tera::Tera::new("templates/**/*")
}

// Handlers

async fn index(tmpl: web::Data<tera::Tera>) -> Result<HttpResponse, Error> {
    let ctx = tera::Context::new();
    let s = tmpl
        .render("index.html", &ctx)
        .map_err(|_| error::ErrorInternalServerError("template error"))?;

    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

// API routes

#[derive(Deserialize, Serialize, Debug)]
struct DownloadTimeseriesRequest {
    name: String,
    version: Option<String>,
}

async fn download_timeseries(
    item: web::Json<DownloadTimeseriesRequest>,
    db: web::Data<r2d2::Pool<r2d2_postgres::PostgresConnectionManager>>,
) -> Result<HttpResponse, Error> {
    let req = item.0;

    #[derive(Serialize)]
    struct Response {
        name: String,
        version: Option<String>,
        downloads: Vec<Download>,
    }

    #[derive(Serialize)]
    struct Download {
        date: chrono::NaiveDate,
        downloads: i64,
    }

    // execute sync code in threadpool
    let res = web::block(move || {
        let conn = db.get().unwrap();

        let rows = if let Some(version) = req.version.as_ref() {
            conn.query(
                "
            SELECT version_downloads.date, sum(version_downloads.downloads)
            FROM crates
            JOIN versions ON crates.id = versions.crate_id
            JOIN version_downloads ON versions.id = version_downloads.version_id
            WHERE crates.name = $1
            AND versions.num = $2
            GROUP BY version_downloads.date
            ORDER BY version_downloads.date ASC",
                &[&req.name, &version],
            )
            .unwrap()
        } else {
            conn.query(
                "
            SELECT version_downloads.date, sum(version_downloads.downloads)
            FROM crates
            JOIN versions ON crates.id = versions.crate_id
            JOIN version_downloads ON versions.id = version_downloads.version_id
            WHERE crates.name = $1
            GROUP BY version_downloads.date
            ORDER BY version_downloads.date ASC",
                &[&req.name],
            )
            .unwrap()
        };

        let downloads = rows
            .iter()
            .map(|row| Download {
                date: row.get(0),
                downloads: row.get(1),
            })
            .collect();

        let res: Result<Response, ()> = Ok(Response {
            name: req.name.clone(),
            version: req.version.clone(),
            downloads,
        });
        res
    })
    .await
    .map(|v| HttpResponse::Ok().json(v))
    .map_err(|_| HttpResponse::InternalServerError())?;

    Ok(res)
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    // Initial setup
    dotenv::dotenv().ok();
    env_logger::init();
    let mut listenfd = ListenFd::from_env();

    // Get variables from the environment
    let db_conn_str =
        std::env::var("DATABASE_URL").expect("DATABASE_URL variable not set (see .env file)");
    let port = std::env::var("PORT").expect("PORT variable not set (see .env file)");

    // Set up the database
    let manager = r2d2_postgres::PostgresConnectionManager::new(db_conn_str, TlsMode::None)
        .expect("creating postgres connection manager");
    let pool = r2d2::Pool::new(manager).expect("setting up postgres connection pool");

    let mut server = HttpServer::new(move || {
        let tera = set_up_tera().expect("could not set up tera");

        App::new()
            .wrap(middleware::Logger::default())
            .service(
                web::scope("/api/v1")
                    // Limit the size of incoming payload
                    .data(web::JsonConfig::default().limit(1024))
                    .data(pool.clone())
                    .route("/downloads", web::post().to(download_timeseries)),
            )
            .service(fs::Files::new("/static", "static"))
            .service(web::scope("/").data(tera).route("", web::get().to(index)))
    });

    // Let listenfd support live reloading
    server = if let Some(l) = listenfd.take_tcp_listener(0).unwrap() {
        server.listen(l)?
    } else {
        server.bind(format!("127.0.0.1:{port}", port = port))?
    };

    server.start().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::dev::Service;
    use actix_web::{http, test, web, App};

    #[actix_rt::test]
    async fn test_index() -> Result<(), Error> {
        // Set up app
        let tera = set_up_tera().unwrap();

        let mut app = test::init_service(
            App::new()
                .data(tera)
                .service(web::resource("/").route(web::get().to(index))),
        )
        .await;

        // Send request
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = app.call(req).await?;

        assert_eq!(resp.status(), http::StatusCode::OK);

        Ok(())
    }
}
