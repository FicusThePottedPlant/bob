pub(crate) use super::prelude::*;

pub mod http;

pub mod grpc {
    tonic::include_proto!("bob_storage");
}

pub mod prelude {
    pub(crate) use super::*;
    pub(crate) use rocket::{http::RawStr, request::FromParam, State};
    pub(crate) use rocket_contrib::json::Json;
    pub(crate) use server::BobSrv;
}
