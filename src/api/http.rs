use super::prelude::*;

#[derive(Debug, Clone)]
pub enum Action {
    Attach,
    Detach,
}

#[derive(Debug, Serialize)]
pub struct Node {
    name: String,
    address: String,
    vdisks: Vec<VDisk>,
}

#[derive(Debug, Serialize)]
pub struct VDisk {
    id: u32,
    replicas: Vec<Replica>,
}

#[derive(Debug, Serialize)]
pub struct Replica {
    node: String,
    disk: String,
    path: String,
}
#[derive(Debug, Serialize, Clone)]
pub struct VDiskPartitions {
    vdisk_id: u32,
    node_name: String,
    disk_name: String,
    partitions: Vec<i64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Partition {
    vdisk_id: u32,
    node_name: String,
    disk_name: String,
    timestamp: i64,
}

#[derive(Debug)]
pub struct StatusExt {
    status: Status,
    ok: bool,
    msg: String,
}

pub fn spawn(bob: BobSrv, port: u16) {
    let routes = routes![
        status,
        vdisks,
        vdisk_by_id,
        partitions,
        partition_by_id,
        change_partition_state,
        alien
    ];
    let task = move || {
        info!("API server started");
        let mut config = Config::production();
        config.set_port(port);
        Rocket::custom(config)
            .manage(bob)
            .mount("/", routes)
            .launch();
    };
    thread::spawn(task);
}

fn data_vdisk_to_scheme(disk: &DataVDisk) -> VDisk {
    VDisk {
        id: disk.id.as_u32(),
        replicas: collect_replicas_info(&disk.replicas),
    }
}

fn collect_disks_info(bob: &BobSrv) -> Vec<VDisk> {
    let mapper = bob.grinder.backend.mapper();
    mapper.vdisks().iter().map(data_vdisk_to_scheme).collect()
}

#[inline]
fn get_vdisk_by_id(bob: &BobSrv, id: u32) -> Option<VDisk> {
    find_vdisk(bob, id).map(data_vdisk_to_scheme)
}

fn find_vdisk(bob: &BobSrv, id: u32) -> Option<&DataVDisk> {
    let mapper = bob.grinder.backend.mapper();
    mapper.vdisks().iter().find(|disk| disk.id.as_u32() == id)
}

fn collect_replicas_info(replicas: &[DataNodeDisk]) -> Vec<Replica> {
    replicas
        .iter()
        .map(|r| Replica {
            path: r.disk_path.to_owned(),
            disk: r.disk_name.to_owned(),
            node: r.node_name.to_owned(),
        })
        .collect()
}

fn not_acceptable_backend() -> Status {
    let mut status = Status::NotAcceptable;
    status.reason = "only pearl backend supports partitions";
    warn!("{:?}", status);
    status
}

fn find_group(bob: &BobSrv, vdisk_id: u32) -> Result<&PearlGroup, StatusExt> {
    let backend = bob.grinder.backend.backend();
    debug!("get backend: OK");
    let groups = backend.vdisks_groups().ok_or_else(not_acceptable_backend)?;
    debug!("get vdisks groups: OK");
    groups
        .iter()
        .find(|group| group.vdisk_id() == vdisk_id)
        .ok_or_else(|| {
            let err = format!("vdisk with id: {} not found", vdisk_id);
            warn!("{}", err);
            StatusExt::new(Status::NotFound, false, err)
        })
}

#[get("/status")]
fn status(bob: State<BobSrv>) -> Json<Node> {
    let mapper = bob.grinder.backend.mapper();
    let name = mapper.local_node_name().to_owned();
    let address = mapper.local_node_address();
    let vdisks = collect_disks_info(&bob);
    let node = Node {
        name,
        address,
        vdisks,
    };
    Json(node)
}

#[get("/vdisks")]
fn vdisks(bob: State<BobSrv>) -> Json<Vec<VDisk>> {
    let vdisks = collect_disks_info(&bob);
    Json(vdisks)
}

#[get("/vdisks/<vdisk_id>")]
fn vdisk_by_id(bob: State<BobSrv>, vdisk_id: u32) -> Option<Json<VDisk>> {
    get_vdisk_by_id(&bob, vdisk_id).map(Json)
}

#[get("/vdisks/<vdisk_id>/partitions")]
fn partitions(bob: State<BobSrv>, vdisk_id: u32) -> Result<Json<VDiskPartitions>, StatusExt> {
    let group = find_group(&bob, vdisk_id)?;
    debug!("group with provided vdisk_id found");
    let pearls = group.pearls().ok_or_else(|| {
        error!("writer panics while holding an exclusive lock");
        Status::InternalServerError
    })?;
    debug!("get pearl holders: OK");
    let pearls: &[_] = pearls.as_ref();
    let partitions = pearls.iter().map(|pearl| pearl.start_timestamp).collect();
    let ps = VDiskPartitions {
        node_name: group.node_name().to_owned(),
        disk_name: group.disk_name().to_owned(),
        vdisk_id: group.vdisk_id(),
        partitions,
    };
    trace!("partitions: {:?}", ps);
    Ok(Json(ps))
}

#[get("/vdisks/<vdisk_id>/partitions/<partition_id>")]
fn partition_by_id(
    bob: State<BobSrv>,
    vdisk_id: u32,
    partition_id: i64,
) -> Result<Json<Partition>, StatusExt> {
    let group = find_group(&bob, vdisk_id)?;
    debug!("group with provided vdisk_id found");
    let pearls = group.pearls().ok_or_else(|| {
        error!("writer panics while holding an exclusive lock");
        Status::InternalServerError
    })?;
    debug!("get pearl holders: OK");
    let pearls: &[_] = pearls.as_ref();
    pearls
        .iter()
        .map(|pearl| pearl.start_timestamp)
        .find_map(|timestamp| {
            if timestamp == partition_id {
                Some(Partition {
                    node_name: group.node_name().to_owned(),
                    disk_name: group.disk_name().to_owned(),
                    vdisk_id: group.vdisk_id(),
                    timestamp,
                })
            } else {
                None
            }
        })
        .map(Json)
        .ok_or_else(|| {
            let err = format!(
                "partition with id: {} in vdisk {} not found",
                partition_id, vdisk_id
            );
            warn!("{}", err);
            StatusExt::new(Status::NotFound, false, err)
        })
}

#[post("/vdisks/<vdisk_id>/partitions/<partition_id>/<action>")]
fn change_partition_state(
    bob: State<BobSrv>,
    vdisk_id: u32,
    partition_id: i64,
    action: Action,
) -> Result<StatusExt, StatusExt> {
    let group = find_group(&bob, vdisk_id)?;
    let group = group.clone();
    // TODO: run web server on same runtime as bob
    error!("HOT FIX: run web server on same runtime as bob");
    let mut rt = Runtime::new().expect("create runtime");
    let res = format!(
        "partition with id: {} in vdisk {} is successfully {:?}ed",
        partition_id, vdisk_id, action
    );
    let task = async move {
        match action {
            Action::Attach => group.attach(partition_id).await,
            Action::Detach => group.detach(partition_id).await,
        }
    };
    match rt.block_on(task) {
        Ok(_) => {
            info!("{}", res);
            Ok(StatusExt::new(Status::Ok, true, res))
        }
        Err(e) => Err(StatusExt::new(Status::Ok, false, e.to_string())),
    }
}

#[get("/alien")]
fn alien(_bob: State<BobSrv>) -> &'static str {
    "alien"
}

impl<'r> FromParam<'r> for Action {
    type Error = &'r RawStr;

    fn from_param(param: &'r RawStr) -> Result<Self, Self::Error> {
        error!("{}", param.as_str());
        match param.as_str() {
            "attach" => Ok(Self::Attach),
            "detach" => Ok(Self::Detach),
            _ => Err(param),
        }
    }
}

impl Responder<'_> for StatusExt {
    fn respond_to(self, _: &Request) -> RocketResult<'static> {
        let msg = format!("{{ \"ok\": {}, \"msg\": \"{}\" }}", self.ok, self.msg);
        Response::build()
            .status(self.status)
            .sized_body(Cursor::new(msg))
            .ok()
    }
}

impl StatusExt {
    fn new(status: Status, ok: bool, msg: String) -> Self {
        Self { status, ok, msg }
    }
}

impl From<Status> for StatusExt {
    fn from(status: Status) -> Self {
        Self {
            status,
            ok: true,
            msg: status.reason.to_owned(),
        }
    }
}