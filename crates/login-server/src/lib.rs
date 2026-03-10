//! TCP login server implementation.

use common::{config::NetworkConfig, error::AppError};
use database::Database;
use protocol::messages::{Request, Response};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
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

async fn handle(mut stream: TcpStream, db: Database, supernode: String) -> Result<(), AppError> {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        reader.read_line(&mut line).await?;
    }

    let req: Request = serde_json::from_str(line.trim())?;
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
                    assigned_supernode: supernode,
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

    let body = serde_json::to_vec(&resp)?;
    stream.write_all(&body).await?;
    stream.write_all(b"\n").await?;
    Ok(())
}
