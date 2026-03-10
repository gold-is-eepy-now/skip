//! TCP login server implementation.

use common::{config::NetworkConfig, error::AppError};
use database::Database;
use protocol::{
    codec::{read_json, write_json},
    messages::{Request, Response},
};
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
};

/// Runs the login server event loop.
pub async fn run(config: NetworkConfig, db_url: &str) -> Result<(), AppError> {
    let bind_addr = format!("{}:{}", config.login_host, config.login_port);
    let listener = TcpListener::bind(&bind_addr).await?;
    let db = Database::connect(db_url).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let db = db.clone();
        let assigned_supernode = format!("{}:{}", config.supernode_host, config.supernode_port);
        tokio::spawn(async move {
            if let Err(e) = handle(stream, db, assigned_supernode).await {
                eprintln!("login-server connection error: {e}");
            }
        });
    }
}

async fn handle(stream: TcpStream, db: Database, supernode: String) -> Result<(), AppError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    loop {
        let req = match read_json::<Request, _>(&mut reader).await {
            Ok(r) => r,
            Err(AppError::Protocol(msg)) if msg == "connection closed" => break,
            Err(err) => {
                write_json(
                    &mut write_half,
                    &Response::Error {
                        message: format!("invalid request: {err}"),
                    },
                )
                .await?;
                continue;
            }
        };

        let resp = match req {
            Request::Register { username, password } => {
                match db.register_user(&username, &password).await {
                    Ok(()) => Response::RegisterOk,
                    Err(err) => Response::Error {
                        message: format!("register failed: {err}"),
                    },
                }
            }
            Request::Login { username, password } => {
                if db.authenticate(&username, &password).await? {
                    let token = db.create_session(&username).await?;
                    Response::LoginOk {
                        token,
                        assigned_supernode: supernode.clone(),
                    }
                } else {
                    Response::Error {
                        message: "invalid credentials".into(),
                    }
                }
            }
            _ => Response::Error {
                message: "unsupported request on login server".into(),
            },
        };

        write_json(&mut write_half, &resp).await?;
    }

    Ok(())
}
