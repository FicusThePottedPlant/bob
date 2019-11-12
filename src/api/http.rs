use super::prelude::*;

#[derive(Debug)]
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
    partitions: Vec<Partition>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Partition {
    timestamp: i64,
}

pub fn spawn(bob: &BobSrv) {
    let bob = bob.clone();
    thread::spawn(move || {
        info!("API server started");
        rocket::ignite()
            .manage(bob)
            .mount(
                "/",
                routes![
                    status,
                    vdisks,
                    vdisk_by_id,
                    partitions,
                    partition_by_id,
                    change_partition_state,
                    alien
                ],
            )
            .launch();
    });
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
fn partitions(bob: State<BobSrv>, vdisk_id: u32) -> Result<Json<VDiskPartitions>, Status> {
    let backend = &bob.grinder.backend.backend();
    debug!("get backend: OK");
    let groups = backend.vdisks_groups().ok_or_else(|| {
        warn!("only pearl backend supports partitions");
        Status::NotFound
    })?;
    debug!("get vdisks groups: OK");
    let group = groups
        .iter()
        .find(|group| group.vdisk_id() == vdisk_id)
        .ok_or_else(|| {
            warn!("vdisk with id: {} nont found", vdisk_id);
            Status::NotFound
        })?;
    debug!("group with provided vdisk_id found");
    let pearls = group.pearls().ok_or_else(|| {
        error!("writer panics while holding an exclusive lock");
        Status::InternalServerError
    })?;
    debug!("get pearl holders: OK");
    let pearls: &[_] = pearls.as_ref();
    let partitions: Vec<_> = pearls
        .iter()
        .map(|pearl| pearl.start_timestamp)
        .map(|timestamp| Partition { timestamp })
        .collect();
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
fn partition_by_id(_bob: State<BobSrv>, vdisk_id: usize, partition_id: usize) -> String {
    format!("partition {} of vdisk {}", partition_id, vdisk_id)
}

#[put("/vdisks/<vdisk_id>/partitions/<partition_id>/<action>")]
fn change_partition_state(
    _bob: State<BobSrv>,
    vdisk_id: usize,
    partition_id: usize,
    action: Action,
) -> String {
    format!(
        "{:?} partittion {} of vd {}",
        action, partition_id, vdisk_id
    )
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
            "attach" => Ok(Action::Attach),
            "detach" => Ok(Action::Detach),
            _ => Err(param),
        }
    }
}
