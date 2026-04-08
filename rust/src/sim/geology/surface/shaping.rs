use super::*;

#[path = "shaping/erosion.rs"]
mod erosion;
pub(in crate::sim::geology) use erosion::*;

#[path = "shaping/postprocess.rs"]
mod postprocess;
pub(in crate::sim::geology) use postprocess::*;
