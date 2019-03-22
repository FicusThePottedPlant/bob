pub struct BobError {

}


pub struct BobData {
    pub data: Vec<u8>
}

#[derive(Debug)]
pub struct BobKey {
    pub key: u64
}

bitflags! {
    #[derive(Default)]
    pub struct BobOptions: u8 {
        const FORCE_NODE = 0x01;
    }
}


#[derive(Debug, Clone)]
pub struct VDisk {
    pub id: u32,
    pub replicas: Vec<NodeDisk>
}

#[derive(Debug, Clone)]
pub struct Node {
    pub host: String,
    pub port: u16,
}

impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        self.host == other.host && self.port == other.port
    } 
}

#[derive(Debug, Clone)]
pub struct NodeDisk {
    pub node: Node,
    pub path: String
}

impl PartialEq for NodeDisk {
    fn eq(&self, other: &NodeDisk) -> bool {
        self.node == other.node && self.path == other.path
    }
}