// Copyright (C) 2020 O.S. Systems Sofware LTDA
//
// SPDX-License-Identifier: Apache-2.0

use derive_more::From;
use futures_util::lock::Mutex;
use http_client::native::NativeClient;
use std::sync::Arc;

use rwcst::prelude::*;

#[tokio::main]
async fn main() {
    let (url, _guards) = rwcst::start_remote_mock();
    let local_client = LocalClient::new();
    let remote_client = RemoteClient::new(&url);
    let app = App::new(remote_client);
    rwcst::run(local_client, app).await;
}

struct LocalClient {
    client: surf::Client<NativeClient>,
}

struct RemoteClient {
    client: surf::Client<NativeClient>,
    remote: String,
}

struct App {
    info: Arc<Mutex<rwcst::Info>>,
    client: RemoteClient,
}

#[derive(Debug, From)]
enum Err {
    Server(warp::Error),
    Client(surf::Error),
    Parsing(rwcst::ParsingError),
    Io(std::io::Error),
}
type Result<T> = std::result::Result<T, Err>;

#[async_trait::async_trait(?Send)]
impl rwcst::LocalClientImpl for LocalClient {
    type Err = Err;

    fn new() -> Self {
        LocalClient { client: surf::Client::new() }
    }

    async fn fetch_info(&mut self) -> Result<rwcst::Info> {
        Ok(self.client.get("http://localhost:8001").recv_json().await?)
    }
}

#[async_trait::async_trait(?Send)]
impl rwcst::RemoteClientImpl for RemoteClient {
    type Err = Err;

    fn new(remote: &str) -> Self {
        RemoteClient { client: surf::Client::new(), remote: remote.to_owned() }
    }

    async fn fetch_package(&mut self) -> Result<Option<(rwcst::Package, rwcst::Signature)>> {
        let mut response = self.client.get(&self.remote).await?;

        if let surf::http_types::StatusCode::Ok = response.status() {
            let sign =
                rwcst::Signature::from_base64_str(&response.header("signature").unwrap().as_str());
            let pkg = rwcst::Package::parse(&response.body_bytes().await?)?;
            return Ok(Some((pkg, sign)));
        }

        Ok(None)
    }
}

#[async_trait::async_trait(?Send)]
impl rwcst::AppImpl for App {
    type Err = Err;
    type RemoteClient = RemoteClient;

    fn new(client: RemoteClient) -> Self {
        let info = Arc::default();
        App { info, client }
    }

    fn serve(&mut self) -> Result<()> {
        use std::ops::Deref;
        use warp::{reject::Rejection, reply::Json, Filter};
        type Result = std::result::Result<Json, Rejection>;

        let state = self.info.clone();
        let route = warp::get().and_then(move || {
            let state = state.clone();
            async move {
                let state = state.lock().await;
                Result::Ok(warp::reply::json(state.deref()))
            }
        });

        tokio::spawn(warp::serve(route).run(([127, 0, 0, 1], 8001)));

        Ok(())
    }

    async fn map_info<F: FnOnce(&mut rwcst::Info)>(&mut self, f: F) -> Result<()> {
        use std::ops::DerefMut;
        Ok(f(self.info.lock().await.deref_mut()))
    }

    async fn client(&mut self) -> Result<&mut RemoteClient> {
        Ok(&mut self.client)
    }
}