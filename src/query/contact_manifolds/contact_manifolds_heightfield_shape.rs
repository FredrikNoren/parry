use crate::bounding_volume::BoundingVolume;
use crate::math::{Isometry, Real};
use crate::query::contact_manifolds::ContactManifoldsWorkspace;
use crate::query::query_dispatcher::PersistentQueryDispatcher;
use crate::query::ContactManifold;
#[cfg(feature = "dim2")]
use crate::shape::Capsule;
use crate::shape::{HeightField, Shape};
use crate::utils::MaybeSerializableData;
#[cfg(feature = "serde-serialize")]
use erased_serde::Serialize;
use std::collections::{hash_map::Entry, HashMap};

#[cfg_attr(feature = "serde-serialize", derive(Serialize, Deserialize))]
#[derive(Clone)]
struct SubDetector {
    manifold_id: usize,
    timestamp: bool,
}

#[cfg_attr(feature = "serde-serialize", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub struct HeightFieldShapeContactManifoldsWorkspace {
    timestamp: bool,
    sub_detectors: HashMap<usize, SubDetector>,
}

impl HeightFieldShapeContactManifoldsWorkspace {
    pub fn new() -> Self {
        Self {
            timestamp: false,
            sub_detectors: HashMap::default(),
        }
    }
}

pub fn contact_manifolds_heightfield_shape_shapes<ManifoldData, ContactData>(
    dispatcher: &dyn PersistentQueryDispatcher<ManifoldData, ContactData>,
    pos12: &Isometry<Real>,
    shape1: &dyn Shape,
    shape2: &dyn Shape,
    prediction: Real,
    manifolds: &mut Vec<ContactManifold<ManifoldData, ContactData>>,
    workspace: &mut Option<ContactManifoldsWorkspace>,
) where
    ManifoldData: Default + Clone,
    ContactData: Default + Copy,
{
    if let Some(heightfield1) = shape1.as_heightfield() {
        contact_manifolds_heightfield_shape(
            dispatcher,
            pos12,
            heightfield1,
            shape2,
            prediction,
            manifolds,
            workspace,
            false,
        )
    } else if let Some(heightfield2) = shape2.as_heightfield() {
        contact_manifolds_heightfield_shape(
            dispatcher,
            &pos12.inverse(),
            heightfield2,
            shape1,
            prediction,
            manifolds,
            workspace,
            true,
        )
    }
}

fn ensure_workspace_exists(workspace: &mut Option<ContactManifoldsWorkspace>) {
    if workspace
        .as_ref()
        .and_then(|w| {
            w.0.downcast_ref::<HeightFieldShapeContactManifoldsWorkspace>()
        })
        .is_some()
    {
        return;
    }

    *workspace = Some(ContactManifoldsWorkspace(Box::new(
        HeightFieldShapeContactManifoldsWorkspace::new(),
    )));
}

pub fn contact_manifolds_heightfield_shape<ManifoldData, ContactData>(
    dispatcher: &dyn PersistentQueryDispatcher<ManifoldData, ContactData>,
    pos12: &Isometry<Real>,
    heightfield1: &HeightField,
    shape2: &dyn Shape,
    prediction: Real,
    manifolds: &mut Vec<ContactManifold<ManifoldData, ContactData>>,
    workspace: &mut Option<ContactManifoldsWorkspace>,
    flipped: bool,
) where
    ManifoldData: Default + Clone,
    ContactData: Default + Copy,
{
    ensure_workspace_exists(workspace);
    let workspace: &mut HeightFieldShapeContactManifoldsWorkspace =
        workspace.as_mut().unwrap().0.downcast_mut().unwrap();
    let new_timestamp = !workspace.timestamp;
    workspace.timestamp = new_timestamp;

    /*
     * Compute interferences.
     */
    // TODO: somehow precompute the AABB and reuse it?
    let ls_aabb2 = shape2.compute_aabb(&pos12).loosened(prediction);
    let mut old_manifolds = std::mem::replace(manifolds, Vec::new());

    heightfield1.map_elements_in_local_aabb(&ls_aabb2, &mut |i, part1| {
        #[cfg(feature = "dim2")]
        let sub_shape1 = Capsule::new(part1.a, part1.b, 0.0); // TODO: use a segment instead.
        #[cfg(feature = "dim3")]
        let sub_shape1 = *part1;

        let sub_detector = match workspace.sub_detectors.entry(i as usize) {
            Entry::Occupied(entry) => {
                let sub_detector = entry.into_mut();
                let manifold = old_manifolds[sub_detector.manifold_id].take();
                sub_detector.manifold_id = manifolds.len();
                sub_detector.timestamp = new_timestamp;
                manifolds.push(manifold);
                sub_detector
            }
            Entry::Vacant(entry) => {
                let sub_detector = SubDetector {
                    manifold_id: manifolds.len(),
                    timestamp: new_timestamp,
                };

                let (id1, id2) = if flipped { (0, i) } else { (i, 0) };
                manifolds.push(ContactManifold::with_data(
                    id1,
                    id2,
                    ManifoldData::default(),
                ));

                entry.insert(sub_detector)
            }
        };

        let manifold = &mut manifolds[sub_detector.manifold_id];

        if flipped {
            let _ = dispatcher.contact_manifold_convex_convex(
                &pos12.inverse(),
                shape2,
                &sub_shape1,
                prediction,
                manifold,
            );
        } else {
            let _ = dispatcher.contact_manifold_convex_convex(
                pos12,
                &sub_shape1,
                shape2,
                prediction,
                manifold,
            );
        }
    });

    workspace
        .sub_detectors
        .retain(|_, detector| detector.timestamp == new_timestamp)
}

impl MaybeSerializableData for HeightFieldShapeContactManifoldsWorkspace {
    #[cfg(feature = "serde-serialize")]
    fn as_serialize(&self) -> Option<(u32, &dyn Serialize)> {
        Some((
            super::WorkspaceSerializationTag::HeightfieldShapeContactManifoldsWorkspace as u32,
            self,
        ))
    }

    fn clone_dyn(&self) -> Box<dyn MaybeSerializableData> {
        Box::new(self.clone())
    }
}
