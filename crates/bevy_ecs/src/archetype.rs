//! Types for defining [`Archetype`]s, collections of entities that have the same set of
//! components.

use crate::{
    bundle::BundleId,
    component::{ComponentId, StorageType},
    entity::{Entity, EntityLocation},
    storage::{ImmutableSparseSet, SparseArray, SparseSet, SparseSetIndex, TableId},
};
use nonmax::NonMaxU32;
use std::{
    collections::HashMap,
    hash::Hash,
    ops::{Index, IndexMut},
};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ArchetypeId(NonMaxU32);

impl ArchetypeId {
    pub const EMPTY: ArchetypeId = unsafe { ArchetypeId::new_unchecked(0) };

    /// Creates a new [`ArchetypeId`].
    ///
    /// # Panics
    /// This will panic if `index` is `u32::MAX` or greater.
    #[inline]
    pub fn new(index: usize) -> Self {
        assert!(
            index < u32::MAX as usize,
            "ArchetypeID cannot be u32::MAX or greater"
        );
        // SAFE: The above assertion will fail if the value is not valid.
        unsafe { Self(NonMaxU32::new_unchecked(index as u32)) }
    }

    /// Creates a new [`ArchetypeId`] without checking for validity of the
    /// provided index.
    ///
    /// # Safety
    /// This is only safe if `index` is not `u32::MAX` or greater.
    #[inline]
    pub const unsafe fn new_unchecked(index: usize) -> Self {
        Self(NonMaxU32::new_unchecked(index as u32))
    }

    /// Gets the corresponding index for the ID.
    #[inline]
    pub const fn index(self) -> usize {
        self.0.get() as usize
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ComponentStatus {
    Added,
    Mutated,
}

pub struct AddBundle {
    pub archetype_id: ArchetypeId,
    pub(crate) bundle_status: Vec<ComponentStatus>,
}

/// This trait is used to report the status of [`Bundle`](crate::bundle::Bundle) components
/// being added to a given entity, relative to that entity's original archetype.
/// See [`crate::bundle::BundleInfo::write_components`] for more info.
pub(crate) trait BundleComponentStatus {
    /// Returns the Bundle's component status for the given "bundle index"
    ///
    /// # Safety
    /// Callers must ensure that index is always a valid bundle index for the
    /// Bundle associated with this [`BundleComponentStatus`]
    unsafe fn get_status(&self, index: usize) -> ComponentStatus;
}

impl BundleComponentStatus for AddBundle {
    #[inline]
    unsafe fn get_status(&self, index: usize) -> ComponentStatus {
        // SAFETY: caller has ensured index is a valid bundle index for this bundle
        *self.bundle_status.get_unchecked(index)
    }
}

pub(crate) struct SpawnBundleStatus;

impl BundleComponentStatus for SpawnBundleStatus {
    #[inline]
    unsafe fn get_status(&self, _index: usize) -> ComponentStatus {
        // Components added during a spawn_bundle call are always treated as added
        ComponentStatus::Added
    }
}

/// Archetypes and bundles form a graph. Adding or removing a bundle moves
/// an [`Entity`] to a new [`Archetype`].
///
/// [`Edges`] caches the results of these moves. Each archetype caches
/// the result of a structural alteration. This can be used to monitor the
/// state of the archetype graph.
///
/// Note: This type only contains edges the [`World`] has already traversed.
/// If any of functions return `None`, it doesn't mean there is guaranteed
/// not to be a result of adding or removing that bundle, but rather that
/// operation that has moved an entity along that edge has not been performed
/// yet.
///
/// [`World`]: crate::world::World
#[derive(Default)]
pub struct Edges {
    add_bundle: SparseArray<BundleId, AddBundle>,
    remove_bundle: SparseArray<BundleId, Option<ArchetypeId>>,
    remove_bundle_intersection: SparseArray<BundleId, Option<ArchetypeId>>,
}

impl Edges {
    #[inline]
    pub fn get_add_bundle(&self, bundle_id: BundleId) -> Option<&AddBundle> {
        self.add_bundle.get(bundle_id)
    }

    #[inline]
    pub(crate) fn insert_add_bundle(
        &mut self,
        bundle_id: BundleId,
        archetype_id: ArchetypeId,
        bundle_status: Vec<ComponentStatus>,
    ) {
        self.add_bundle.insert(
            bundle_id,
            AddBundle {
                archetype_id,
                bundle_status,
            },
        );
    }

    #[inline]
    pub fn get_remove_bundle(&self, bundle_id: BundleId) -> Option<Option<ArchetypeId>> {
        self.remove_bundle.get(bundle_id).cloned()
    }

    #[inline]
    pub(crate) fn insert_remove_bundle(
        &mut self,
        bundle_id: BundleId,
        archetype_id: Option<ArchetypeId>,
    ) {
        self.remove_bundle.insert(bundle_id, archetype_id);
    }

    #[inline]
    pub fn get_remove_bundle_intersection(
        &self,
        bundle_id: BundleId,
    ) -> Option<Option<ArchetypeId>> {
        self.remove_bundle_intersection.get(bundle_id).cloned()
    }

    #[inline]
    pub(crate) fn insert_remove_bundle_intersection(
        &mut self,
        bundle_id: BundleId,
        archetype_id: Option<ArchetypeId>,
    ) {
        self.remove_bundle_intersection
            .insert(bundle_id, archetype_id);
    }
}

pub struct ArchetypeEntity {
    pub(crate) entity: Entity,
    pub(crate) table_row: usize,
}

impl ArchetypeEntity {
    pub fn entity(&self) -> Entity {
        self.entity
    }

    pub fn table_row(&self) -> usize {
        self.table_row
    }
}

pub(crate) struct ArchetypeSwapRemoveResult {
    pub(crate) swapped_entity: Option<Entity>,
    pub(crate) table_row: usize,
}

pub(crate) struct ArchetypeComponentInfo {
    pub(crate) storage_type: StorageType,
    pub(crate) archetype_component_id: ArchetypeComponentId,
}

pub struct Archetype {
    id: ArchetypeId,
    table_id: TableId,
    edges: Edges,
    entities: Vec<ArchetypeEntity>,
    components: ImmutableSparseSet<ComponentId, ArchetypeComponentInfo>,
}

impl Archetype {
    pub fn new(
        id: ArchetypeId,
        table_id: TableId,
        table_components: impl Iterator<Item = (ComponentId, ArchetypeComponentId)>,
        sparse_set_components: impl Iterator<Item = (ComponentId, ArchetypeComponentId)>,
    ) -> Self {
        let (min_table, _) = table_components.size_hint();
        let (min_sparse, _) = sparse_set_components.size_hint();
        let mut components = SparseSet::with_capacity(min_table + min_sparse);
        for (component_id, archetype_component_id) in table_components {
            components.insert(
                component_id,
                ArchetypeComponentInfo {
                    storage_type: StorageType::Table,
                    archetype_component_id,
                },
            );
        }

        for (component_id, archetype_component_id) in sparse_set_components {
            components.insert(
                component_id,
                ArchetypeComponentInfo {
                    storage_type: StorageType::SparseSet,
                    archetype_component_id,
                },
            );
        }
        Self {
            id,
            table_id,
            entities: Vec::new(),
            components: components.into_immutable(),
            edges: Default::default(),
        }
    }

    #[inline]
    pub fn id(&self) -> ArchetypeId {
        self.id
    }

    #[inline]
    pub fn table_id(&self) -> TableId {
        self.table_id
    }

    #[inline]
    pub fn entities(&self) -> &[ArchetypeEntity] {
        &self.entities
    }

    #[inline]
    pub fn table_components(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.components
            .iter()
            .filter(|(_, component)| component.storage_type == StorageType::Table)
            .map(|(id, _)| *id)
    }

    #[inline]
    pub fn sparse_set_components(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.components
            .iter()
            .filter(|(_, component)| component.storage_type == StorageType::SparseSet)
            .map(|(id, _)| *id)
    }

    #[inline]
    pub fn components(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.components.indices()
    }

    #[inline]
    pub fn edges(&self) -> &Edges {
        &self.edges
    }

    #[inline]
    pub(crate) fn edges_mut(&mut self) -> &mut Edges {
        &mut self.edges
    }

    #[inline]
    pub fn entity_table_row(&self, index: usize) -> usize {
        self.entities[index].table_row
    }

    #[inline]
    pub(crate) fn set_entity_table_row(&mut self, index: usize, table_row: usize) {
        self.entities[index].table_row = table_row;
    }

    /// # Safety
    /// valid component values must be immediately written to the relevant storages
    /// `table_row` must be valid
    pub(crate) unsafe fn allocate(&mut self, entity: Entity, table_row: usize) -> EntityLocation {
        self.entities.push(ArchetypeEntity { entity, table_row });

        EntityLocation {
            archetype_id: self.id,
            index: self.entities.len() - 1,
        }
    }

    pub(crate) fn reserve(&mut self, additional: usize) {
        self.entities.reserve(additional);
    }

    /// Removes the entity at `index` by swapping it out. Returns the table row the entity is stored
    /// in.
    pub(crate) fn swap_remove(&mut self, index: usize) -> ArchetypeSwapRemoveResult {
        let is_last = index == self.entities.len() - 1;
        let entity = self.entities.swap_remove(index);
        ArchetypeSwapRemoveResult {
            swapped_entity: if is_last {
                None
            } else {
                Some(self.entities[index].entity)
            },
            table_row: entity.table_row,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    #[inline]
    pub fn contains(&self, component_id: ComponentId) -> bool {
        self.components.contains(component_id)
    }

    #[inline]
    pub fn get_storage_type(&self, component_id: ComponentId) -> Option<StorageType> {
        self.components
            .get(component_id)
            .map(|info| info.storage_type)
    }

    #[inline]
    pub fn get_archetype_component_id(
        &self,
        component_id: ComponentId,
    ) -> Option<ArchetypeComponentId> {
        self.components
            .get(component_id)
            .map(|info| info.archetype_component_id)
    }

    pub(crate) fn clear_entities(&mut self) {
        self.entities.clear();
    }
}

/// A generational id that changes every time the set of archetypes changes
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ArchetypeGeneration(usize);

impl ArchetypeGeneration {
    #[inline]
    pub const fn initial() -> Self {
        ArchetypeGeneration(0)
    }

    #[inline]
    pub fn value(self) -> usize {
        self.0
    }
}

#[derive(Hash, PartialEq, Eq)]
pub struct ArchetypeIdentity {
    table_components: Box<[ComponentId]>,
    sparse_set_components: Box<[ComponentId]>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ArchetypeComponentId(NonMaxU32);

impl ArchetypeComponentId {
    /// Creates a new [`ArchetypeComponentId`] from an index without
    /// checking for the type's invariants.
    ///
    /// # Safety
    /// `index` must not be [`usize::MAX`].
    #[inline]
    pub const unsafe fn new_unchecked(index: usize) -> Self {
        Self(NonMaxU32::new_unchecked(index as u32))
    }

    /// Creates a new [`ArchetypeComponentId`] from an index.
    ///
    /// # Panic
    /// This function will panic if `index` is equal to [`usize::MAX`].
    #[inline]
    pub const fn new(index: usize) -> Self {
        assert!(
            index < u32::MAX as usize,
            "ArchetypeComponentId cannot be u32::MAX or greater"
        );
        // SAFE: The above assertion will fail if the value is not valid.
        unsafe { Self::new_unchecked(index) }
    }

    #[inline]
    pub fn index(self) -> usize {
        self.0.get() as usize
    }
}

impl SparseSetIndex for ArchetypeComponentId {
    type Repr = NonMaxU32;

    #[inline]
    fn sparse_set_index(&self) -> usize {
        self.0.get() as usize
    }

    #[inline]
    fn get_sparse_set_index(value: usize) -> Self {
        Self::new(value)
    }

    #[inline]
    fn repr_from_index(index: usize) -> Self::Repr {
        NonMaxU32::new(index as u32).unwrap()
    }

    #[inline]
    fn repr_to_index(repr: &Self::Repr) -> usize {
        repr.get() as usize
    }
}

pub struct Archetypes {
    pub(crate) archetypes: Vec<Archetype>,
    pub(crate) archetype_component_count: usize,
    archetype_ids: HashMap<ArchetypeIdentity, ArchetypeId>,
}

impl Default for Archetypes {
    fn default() -> Self {
        let mut archetypes = Archetypes {
            archetypes: Vec::new(),
            archetype_ids: Default::default(),
            archetype_component_count: 0,
        };
        archetypes.get_id_or_insert(TableId::empty(), Vec::new(), Vec::new());
        archetypes
    }
}

impl Archetypes {
    #[inline]
    pub fn generation(&self) -> ArchetypeGeneration {
        ArchetypeGeneration(self.archetypes.len())
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.archetypes.len()
    }

    #[inline]
    pub fn empty(&self) -> &Archetype {
        // SAFETY: empty archetype always exists
        unsafe { self.archetypes.get_unchecked(ArchetypeId::EMPTY.index()) }
    }

    #[inline]
    pub(crate) fn empty_mut(&mut self) -> &mut Archetype {
        // SAFETY: empty archetype always exists
        unsafe {
            self.archetypes
                .get_unchecked_mut(ArchetypeId::EMPTY.index())
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.archetypes.is_empty()
    }

    #[inline]
    pub fn get(&self, id: ArchetypeId) -> Option<&Archetype> {
        self.archetypes.get(id.index())
    }

    #[inline]
    pub(crate) fn get_2_mut(
        &mut self,
        a: ArchetypeId,
        b: ArchetypeId,
    ) -> (&mut Archetype, &mut Archetype) {
        if a.index() > b.index() {
            let (b_slice, a_slice) = self.archetypes.split_at_mut(a.index());
            (&mut a_slice[0], &mut b_slice[b.index()])
        } else {
            let (a_slice, b_slice) = self.archetypes.split_at_mut(b.index());
            (&mut a_slice[a.index()], &mut b_slice[0])
        }
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Archetype> {
        self.archetypes.iter()
    }

    /// Gets the archetype id matching the given inputs or inserts a new one if it doesn't exist.
    /// `table_components` and `sparse_set_components` must be sorted
    ///
    /// # Safety
    /// [`TableId`] must exist in tables
    pub(crate) fn get_id_or_insert(
        &mut self,
        table_id: TableId,
        table_components: Vec<ComponentId>,
        sparse_set_components: Vec<ComponentId>,
    ) -> ArchetypeId {
        let archetype_identity = ArchetypeIdentity {
            sparse_set_components: sparse_set_components.clone().into_boxed_slice(),
            table_components: table_components.clone().into_boxed_slice(),
        };

        let archetypes = &mut self.archetypes;
        let archetype_component_count = &mut self.archetype_component_count;
        *self
            .archetype_ids
            .entry(archetype_identity)
            .or_insert_with(move || {
                let id = ArchetypeId::new(archetypes.len());
                let table_start = *archetype_component_count;
                *archetype_component_count += table_components.len();
                let table_archetype_components =
                    (table_start..*archetype_component_count).map(ArchetypeComponentId);
                let sparse_start = *archetype_component_count;
                *archetype_component_count += sparse_set_components.len();
                let sparse_set_archetype_components =
                    (sparse_start..*archetype_component_count).map(ArchetypeComponentId);

                archetypes.push(Archetype::new(
                    id,
                    table_id,
                    table_components.into_iter().zip(table_archetype_components),
                    sparse_set_components
                        .into_iter()
                        .zip(sparse_set_archetype_components),
                ));
                id
            })
    }

    #[inline]
    pub fn archetype_components_len(&self) -> usize {
        self.archetype_component_count
    }

    pub(crate) fn clear_entities(&mut self) {
        for archetype in &mut self.archetypes {
            archetype.clear_entities();
        }
    }
}

impl Index<ArchetypeId> for Archetypes {
    type Output = Archetype;

    #[inline]
    fn index(&self, index: ArchetypeId) -> &Self::Output {
        &self.archetypes[index.index()]
    }
}

impl IndexMut<ArchetypeId> for Archetypes {
    #[inline]
    fn index_mut(&mut self, index: ArchetypeId) -> &mut Self::Output {
        &mut self.archetypes[index.index()]
    }
}

#[cfg(test)]
mod test {
    use super::ArchetypeComponentId;

    #[test]
    pub fn test_archetyype_component_id_size_optimized() {
        assert_eq!(
            core::mem::size_of::<ArchetypeComponentId>(),
            core::mem::size_of::<Option<ArchetypeComponentId>>(),
        );
    }
}
