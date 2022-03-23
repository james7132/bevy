mod conversions;
mod morph_target;

use crate::{
    primitives::Aabb,
    render_asset::{PrepareAssetError, RenderAsset},
    render_resource::{Buffer, VertexBufferLayout, TextureFormat, Extent3d},
    renderer::{RenderDevice, RenderQueue},
    texture::{GpuImage, Image},
};
use bevy_core::cast_slice;
use bevy_ecs::system::{lifetimeless::SRes, SystemParamItem};
use bevy_math::*;
use bevy_reflect::TypeUuid;
pub use morph_target::*;
use bevy_utils::{EnumVariantMeta, Hashed};
use std::{collections::BTreeMap, hash::Hash};
use thiserror::Error;
use wgpu::{
    util::BufferInitDescriptor, BufferUsages, IndexFormat, PrimitiveTopology, VertexAttribute,
    VertexFormat, VertexStepMode,
};

pub const INDEX_BUFFER_ASSET_INDEX: u64 = 0;
pub const VERTEX_ATTRIBUTE_BUFFER_ID: u64 = 10;

// TODO: allow values to be unloaded after been submitting to the GPU to conserve memory
#[derive(Debug, TypeUuid, Clone)]
#[uuid = "8ecbac0f-f545-4473-ad43-e1f4243af51e"]
pub struct Mesh {
    primitive_topology: PrimitiveTopology,
    /// `std::collections::BTreeMap` with all defined vertex attributes (Positions, Normals, ...)
    /// for this mesh. Attribute ids to attribute values.
    /// Uses a BTreeMap because, unlike HashMap, it has a defined iteration order,
    /// which allows easy stable VertexBuffers (i.e. same buffer order)
    attributes: BTreeMap<MeshVertexAttributeId, MeshAttributeData>,
    morph_targets: Vec<MorphTarget>,
    indices: Option<Indices>,
}

/// Contains geometry in the form of a mesh.
///
/// Often meshes are automatically generated by bevy's asset loaders or primitives, such as
/// [`shape::Cube`](crate::mesh::shape::Cube) or [`shape::Box`](crate::mesh::shape::Box), but you can also construct
/// one yourself.
///
/// Example of constructing a mesh:
/// ```
/// # use bevy_render::mesh::{Mesh, Indices};
/// # use bevy_render::render_resource::PrimitiveTopology;
/// fn create_triangle() -> Mesh {
///     let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
///     mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]]);
///     mesh.set_indices(Some(Indices::U32(vec![0,1,2])));
///     mesh
/// }
/// ```
impl Mesh {
    const MORPH_TARGET_PIXEL_SIZE: usize = std::mem::size_of::<Vec4>();

    /// Where the vertex is located in space. Use in conjunction with [`Mesh::insert_attribute`]
    pub const ATTRIBUTE_POSITION: MeshVertexAttribute =
        MeshVertexAttribute::new("Vertex_Position", 0, VertexFormat::Float32x3);

    /// The direction the vertex normal is facing in.
    /// Use in conjunction with [`Mesh::insert_attribute`]
    pub const ATTRIBUTE_NORMAL: MeshVertexAttribute =
        MeshVertexAttribute::new("Vertex_Normal", 1, VertexFormat::Float32x3);

    /// Texture coordinates for the vertex. Use in conjunction with [`Mesh::insert_attribute`]
    pub const ATTRIBUTE_UV_0: MeshVertexAttribute =
        MeshVertexAttribute::new("Vertex_Uv", 2, VertexFormat::Float32x2);

    /// The direction of the vertex tangent. Used for normal mapping
    pub const ATTRIBUTE_TANGENT: MeshVertexAttribute =
        MeshVertexAttribute::new("Vertex_Tangent", 3, VertexFormat::Float32x4);

    /// Per vertex coloring. Use in conjunction with [`Mesh::insert_attribute`]
    pub const ATTRIBUTE_COLOR: MeshVertexAttribute =
        MeshVertexAttribute::new("Vertex_Color", 4, VertexFormat::Uint32);

    /// Per vertex joint transform matrix weight. Use in conjunction with [`Mesh::insert_attribute`]
    pub const ATTRIBUTE_JOINT_WEIGHT: MeshVertexAttribute =
        MeshVertexAttribute::new("Vertex_JointWeight", 5, VertexFormat::Float32x4);
    /// Per vertex joint transform matrix index. Use in conjunction with [`Mesh::insert_attribute`]
    pub const ATTRIBUTE_JOINT_INDEX: MeshVertexAttribute =
        MeshVertexAttribute::new("Vertex_JointIndex", 6, VertexFormat::Uint32);

    /// Construct a new mesh. You need to provide a [`PrimitiveTopology`] so that the
    /// renderer knows how to treat the vertex data. Most of the time this will be
    /// [`PrimitiveTopology::TriangleList`].
    pub fn new(primitive_topology: PrimitiveTopology) -> Self {
        Mesh {
            primitive_topology,
            attributes: Default::default(),
            morph_targets: Vec::new(),
            indices: None,
        }
    }

    /// Returns the topology of the mesh.
    pub fn primitive_topology(&self) -> PrimitiveTopology {
        self.primitive_topology
    }

    /// Sets the data for a vertex attribute (position, normal etc.). The name will
    /// often be one of the associated constants such as [`Mesh::ATTRIBUTE_POSITION`].
    #[inline]
    pub fn insert_attribute(
        &mut self,
        attribute: MeshVertexAttribute,
        values: impl Into<VertexAttributeValues>,
    ) {
        self.attributes.insert(
            attribute.id,
            MeshAttributeData {
                attribute,
                values: values.into(),
            },
        );
    }

    #[inline]
    pub fn contains_attribute(&self, id: impl Into<MeshVertexAttributeId>) -> bool {
        self.attributes.contains_key(&id.into())
    }

    /// Retrieves the data currently set to the vertex attribute with the specified `name`.
    #[inline]
    pub fn attribute(
        &self,
        id: impl Into<MeshVertexAttributeId>,
    ) -> Option<&VertexAttributeValues> {
        self.attributes.get(&id.into()).map(|data| &data.values)
    }

    /// Retrieves the data currently set to the vertex attribute with the specified `name` mutably.
    #[inline]
    pub fn attribute_mut(
        &mut self,
        id: impl Into<MeshVertexAttributeId>,
    ) -> Option<&mut VertexAttributeValues> {
        self.attributes
            .get_mut(&id.into())
            .map(|data| &mut data.values)
    }

    /// Creates a blank new [`MorphTarget`] and returns a mutable reference to it.
    pub fn add_morph_target(&mut self) -> &mut MorphTarget {
        self.morph_targets.push(MorphTarget {
            position_displacement: None,
            normal_displacement: None,
            tangent_displacement: None,
        });
        self.morph_targets.last_mut().unwrap()
    }

    /// Retrieves the morph target at a given index.
    pub fn morph_target(&self, target_idx: usize) -> Option<&MorphTarget> {
        self.morph_targets.get(target_idx)
    }

    /// Retrieves the morph target at a given index mutably.
    pub fn morph_target_mut(&mut self, target_idx: usize) -> Option<&mut MorphTarget> {
        self.morph_targets.get_mut(target_idx)
    }

    /// Sets the vertex indices of the mesh. They describe how triangles are constructed out of the
    /// vertex attributes and are therefore only useful for the [`PrimitiveTopology`] variants
    /// that use triangles.
    #[inline]
    pub fn set_indices(&mut self, indices: Option<Indices>) {
        self.indices = indices;
    }

    /// Retrieves the vertex `indices` of the mesh.
    #[inline]
    pub fn indices(&self) -> Option<&Indices> {
        self.indices.as_ref()
    }

    /// Retrieves the vertex `indices` of the mesh mutably.
    #[inline]
    pub fn indices_mut(&mut self) -> Option<&mut Indices> {
        self.indices.as_mut()
    }

    /// Computes and returns the index data of the mesh as bytes.
    /// This is used to transform the index data into a GPU friendly format.
    pub fn get_index_buffer_bytes(&self) -> Option<&[u8]> {
        self.indices.as_ref().map(|indices| match &indices {
            Indices::U16(indices) => cast_slice(&indices[..]),
            Indices::U32(indices) => cast_slice(&indices[..]),
        })
    }

    /// For a given `descriptor` returns a [`VertexBufferLayout`] compatible with this mesh. If this
    /// mesh is not compatible with the given `descriptor` (ex: it is missing vertex attributes), [`None`] will
    /// be returned.
    pub fn get_mesh_vertex_buffer_layout(&self) -> MeshVertexBufferLayout {
        let mut attributes = Vec::with_capacity(self.attributes.len());
        let mut attribute_ids = Vec::with_capacity(self.attributes.len());
        let mut accumulated_offset = 0;
        for (index, data) in self.attributes.values().enumerate() {
            attribute_ids.push(data.attribute.id);
            attributes.push(VertexAttribute {
                offset: accumulated_offset,
                format: data.attribute.format,
                shader_location: index as u32,
            });
            accumulated_offset += data.attribute.format.get_size();
        }

        MeshVertexBufferLayout::new(InnerMeshVertexBufferLayout {
            layout: VertexBufferLayout {
                array_stride: accumulated_offset,
                step_mode: VertexStepMode::Vertex,
                attributes,
            },
            attribute_ids,
        })
    }

    /// Counts all vertices of the mesh.
    ///
    /// # Panics
    /// Panics if the attributes have different vertex counts.
    pub fn count_vertices(&self) -> usize {
        let mut vertex_count: Option<usize> = None;
        for (attribute_id, attribute_data) in self.attributes.iter() {
            let attribute_len = attribute_data.values.len();
            if let Some(previous_vertex_count) = vertex_count {
                assert_eq!(previous_vertex_count, attribute_len,
                        "{:?} has a different vertex count ({}) than other attributes ({}) in this mesh.", attribute_id, attribute_len, previous_vertex_count);
            }
            vertex_count = Some(attribute_len);
        }

        for (idx, morph_target) in self.morph_targets.iter().enumerate() {
            let morph_target_len = morph_target.count_vertices();
            if let Some(previous_vertex_count) = vertex_count {
                assert_eq!(previous_vertex_count, morph_target_len,
                        "Morph target {} has a different vertex count ({}) than other attributes ({}) in this mesh.", idx, morph_target_len, previous_vertex_count);
            }
            vertex_count = Some(morph_target_len);
        }

        vertex_count.unwrap_or(0)
    }

    /// Computes and returns the vertex data of the mesh as bytes.
    /// Therefore the attributes are located in alphabetical order.
    /// This is used to transform the vertex data into a GPU friendly format.
    ///
    /// # Panics
    /// Panics if the attributes have different vertex counts.
    pub fn get_vertex_buffer_data(&self) -> Vec<u8> {
        let mut vertex_size = 0;
        for attribute_data in self.attributes.values() {
            let vertex_format = attribute_data.attribute.format;
            vertex_size += vertex_format.get_size() as usize;
        }

        let vertex_count = self.count_vertices();
        let mut attributes_interleaved_buffer = vec![0; vertex_count * vertex_size];
        // bundle into interleaved buffers
        let mut attribute_offset = 0;
        for attribute_data in self.attributes.values() {
            let attribute_size = attribute_data.attribute.format.get_size() as usize;
            let attributes_bytes = attribute_data.values.get_bytes();
            for (vertex_index, attribute_bytes) in
                attributes_bytes.chunks_exact(attribute_size).enumerate()
            {
                let offset = vertex_index * vertex_size + attribute_offset;
                attributes_interleaved_buffer[offset..offset + attribute_size]
                    .copy_from_slice(attribute_bytes);
            }

            attribute_offset += attribute_size;
        }

        attributes_interleaved_buffer
    }

    /// Duplicates the vertex attributes so that no vertices are shared.
    ///
    /// This can dramatically increase the vertex count, so make sure this is what you want.
    /// Does nothing if no [Indices] are set.
    ///
    /// # Panics
    /// If the mesh has any other topology than [`PrimitiveTopology::TriangleList`].
    pub fn duplicate_vertices(&mut self) {
        fn duplicate<T: Copy>(values: &[T], indices: impl Iterator<Item = usize>) -> Vec<T> {
            indices.map(|i| values[i]).collect()
        }

        assert!(
            matches!(self.primitive_topology, PrimitiveTopology::TriangleList),
            "can only duplicate vertices for `TriangleList`s"
        );

        let indices = match self.indices.take() {
            Some(indices) => indices,
            None => return,
        };
        for attributes in self.attributes.values_mut() {
            let indices = indices.iter();
            match &mut attributes.values {
                VertexAttributeValues::Float32(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Sint32(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Uint32(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Float32x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Sint32x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Uint32x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Float32x3(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Sint32x3(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Uint32x3(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Sint32x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Uint32x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Float32x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Sint16x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Snorm16x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Uint16x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Unorm16x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Sint16x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Snorm16x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Uint16x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Unorm16x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Sint8x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Snorm8x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Uint8x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Unorm8x2(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Sint8x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Snorm8x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Uint8x4(vec) => *vec = duplicate(vec, indices),
                VertexAttributeValues::Unorm8x4(vec) => *vec = duplicate(vec, indices),
            }
        }
    }

    /// Calculates the [`Mesh::ATTRIBUTE_NORMAL`] of a mesh.
    ///
    /// # Panics
    /// Panics if [`Indices`] are set or [`Mesh::ATTRIBUTE_POSITION`] is not of type `float3`.
    /// Consider calling [`Mesh::duplicate_vertices`] or export your mesh with normal attributes.
    pub fn compute_flat_normals(&mut self) {
        assert!(self.indices().is_none(), "`compute_flat_normals` can't work on indexed geometry. Consider calling `Mesh::duplicate_vertices`.");

        let positions = self
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .expect("`Mesh::ATTRIBUTE_POSITION` vertex attributes should be of type `float3`");

        let normals: Vec<_> = positions
            .chunks_exact(3)
            .map(|p| face_normal(p[0], p[1], p[2]))
            .flat_map(|normal| [normal; 3])
            .collect();

        self.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }

    /// Compute the Axis-Aligned Bounding Box of the mesh vertices in model space
    pub fn compute_aabb(&self) -> Option<Aabb> {
        if let Some(VertexAttributeValues::Float32x3(values)) =
            self.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            let mut minimum = VEC3_MAX;
            let mut maximum = VEC3_MIN;
            for p in values {
                minimum = minimum.min(Vec3::from_slice(p));
                maximum = maximum.max(Vec3::from_slice(p));
            }
            if minimum.x != std::f32::MAX
                && minimum.y != std::f32::MAX
                && minimum.z != std::f32::MAX
                && maximum.x != std::f32::MIN
                && maximum.y != std::f32::MIN
                && maximum.z != std::f32::MIN
            {
                return Some(Aabb::from_min_max(minimum, maximum));
            }
        }

        None
    }

    /// Creates a [`Image`] from the morph target data stored within the mesh.
    ///
    /// Returns `None` if there is no morph target data.
    fn create_morph_target_image(&self) -> Option<Image> {
        if self.morph_targets.is_empty() {
            return None;
        }

        const TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba32Float;
        let vertex_count = self.count_vertices();
        let size = Extent3d {
            width: vertex_count as u32,
            // 3 attributes per morph target: position, normal, tangent.
            height: self.morph_targets.len() as u32 * 3,
            depth_or_array_layers: 1u32,
        };
        let mut data: Vec<u8> =
            vec![0u8; size.width as usize * size.height as usize * Self::MORPH_TARGET_PIXEL_SIZE];
        let row_size = vertex_count * Self::MORPH_TARGET_PIXEL_SIZE;
        let mut offset = 0;
        for morph_target in self.morph_targets.iter() {
            Self::write_morph_target_row(
                morph_target.position_displacement.as_ref(),
                &mut data[offset..offset + row_size],
            );
            offset += row_size;
            Self::write_morph_target_row(
                morph_target.normal_displacement.as_ref(),
                &mut data[offset..offset + row_size],
            );
            offset += row_size;
            Self::write_morph_target_row(
                morph_target.tangent_displacement.as_ref(),
                &mut data[offset..offset + row_size],
            );
            offset += row_size;
        }

        Some(Image::new(size, TextureDimension::D2, data, TEXTURE_FORMAT))
    }

    fn write_morph_target_row(row: Option<&Vec<Vec3>>, dst: &mut [u8]) {
        const VEC3_SIZE: usize = std::mem::size_of::<Vec3>();
        if let Some(row) = row {
            let src: &[u8] = cast_slice(row);
            let src_chunks = src.chunks_exact(VEC3_SIZE);
            let dst_chunks = dst.chunks_exact_mut(Self::MORPH_TARGET_PIXEL_SIZE);
            for (src, dst) in src_chunks.zip(dst_chunks) {
                dst[0..VEC3_SIZE].copy_from_slice(src);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct MeshVertexAttribute {
    /// The friendly name of the vertex attribute
    pub name: &'static str,

    /// The _unique_ id of the vertex attribute. This will also determine sort ordering
    /// when generating vertex buffers. Built-in / standard attributes will use "close to zero"
    /// indices. When in doubt, use a random / very large usize to avoid conflicts.
    pub id: MeshVertexAttributeId,

    /// The format of the vertex attribute.
    pub format: VertexFormat,
}

impl MeshVertexAttribute {
    pub const fn new(name: &'static str, id: usize, format: VertexFormat) -> Self {
        Self {
            name,
            id: MeshVertexAttributeId(id),
            format,
        }
    }

    pub const fn at_shader_location(&self, shader_location: u32) -> VertexAttributeDescriptor {
        VertexAttributeDescriptor::new(shader_location, self.id, self.name)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct MeshVertexAttributeId(usize);

impl From<MeshVertexAttribute> for MeshVertexAttributeId {
    fn from(attribute: MeshVertexAttribute) -> Self {
        attribute.id
    }
}

pub type MeshVertexBufferLayout = Hashed<InnerMeshVertexBufferLayout>;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct InnerMeshVertexBufferLayout {
    attribute_ids: Vec<MeshVertexAttributeId>,
    layout: VertexBufferLayout,
}

impl InnerMeshVertexBufferLayout {
    #[inline]
    pub fn contains(&self, attribute_id: impl Into<MeshVertexAttributeId>) -> bool {
        self.attribute_ids.contains(&attribute_id.into())
    }

    #[inline]
    pub fn attribute_ids(&self) -> &[MeshVertexAttributeId] {
        &self.attribute_ids
    }

    #[inline]
    pub fn layout(&self) -> &VertexBufferLayout {
        &self.layout
    }

    pub fn get_layout(
        &self,
        attribute_descriptors: &[VertexAttributeDescriptor],
    ) -> Result<VertexBufferLayout, MissingVertexAttributeError> {
        let mut attributes = Vec::with_capacity(attribute_descriptors.len());
        for attribute_descriptor in attribute_descriptors.iter() {
            if let Some(index) = self
                .attribute_ids
                .iter()
                .position(|id| *id == attribute_descriptor.id)
            {
                let layout_attribute = &self.layout.attributes[index];
                attributes.push(VertexAttribute {
                    format: layout_attribute.format,
                    offset: layout_attribute.offset,
                    shader_location: attribute_descriptor.shader_location,
                })
            } else {
                return Err(MissingVertexAttributeError {
                    id: attribute_descriptor.id,
                    name: attribute_descriptor.name,
                    pipeline_type: None,
                });
            }
        }

        Ok(VertexBufferLayout {
            array_stride: self.layout.array_stride,
            step_mode: self.layout.step_mode,
            attributes,
        })
    }
}

#[derive(Error, Debug)]
#[error("Mesh is missing requested attribute: {name} ({id:?}, pipeline type: {pipeline_type:?})")]
pub struct MissingVertexAttributeError {
    pub(crate) pipeline_type: Option<&'static str>,
    id: MeshVertexAttributeId,
    name: &'static str,
}

pub struct VertexAttributeDescriptor {
    pub shader_location: u32,
    pub id: MeshVertexAttributeId,
    name: &'static str,
}

impl VertexAttributeDescriptor {
    pub const fn new(shader_location: u32, id: MeshVertexAttributeId, name: &'static str) -> Self {
        Self {
            shader_location,
            id,
            name,
        }
    }
}

#[derive(Debug, Clone)]
struct MeshAttributeData {
    attribute: MeshVertexAttribute,
    values: VertexAttributeValues,
}

const VEC3_MIN: Vec3 = const_vec3!([std::f32::MIN, std::f32::MIN, std::f32::MIN]);
const VEC3_MAX: Vec3 = const_vec3!([std::f32::MAX, std::f32::MAX, std::f32::MAX]);

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let (a, b, c) = (Vec3::from(a), Vec3::from(b), Vec3::from(c));
    (b - a).cross(c - a).normalize().into()
}

pub trait VertexFormatSize {
    fn get_size(self) -> u64;
}

impl VertexFormatSize for wgpu::VertexFormat {
    fn get_size(self) -> u64 {
        match self {
            VertexFormat::Uint8x2 => 2,
            VertexFormat::Uint8x4 => 4,
            VertexFormat::Sint8x2 => 2,
            VertexFormat::Sint8x4 => 4,
            VertexFormat::Unorm8x2 => 2,
            VertexFormat::Unorm8x4 => 4,
            VertexFormat::Snorm8x2 => 2,
            VertexFormat::Snorm8x4 => 4,
            VertexFormat::Uint16x2 => 2 * 2,
            VertexFormat::Uint16x4 => 2 * 4,
            VertexFormat::Sint16x2 => 2 * 2,
            VertexFormat::Sint16x4 => 2 * 4,
            VertexFormat::Unorm16x2 => 2 * 2,
            VertexFormat::Unorm16x4 => 2 * 4,
            VertexFormat::Snorm16x2 => 2 * 2,
            VertexFormat::Snorm16x4 => 2 * 4,
            VertexFormat::Float16x2 => 2 * 2,
            VertexFormat::Float16x4 => 2 * 4,
            VertexFormat::Float32 => 4,
            VertexFormat::Float32x2 => 4 * 2,
            VertexFormat::Float32x3 => 4 * 3,
            VertexFormat::Float32x4 => 4 * 4,
            VertexFormat::Uint32 => 4,
            VertexFormat::Uint32x2 => 4 * 2,
            VertexFormat::Uint32x3 => 4 * 3,
            VertexFormat::Uint32x4 => 4 * 4,
            VertexFormat::Sint32 => 4,
            VertexFormat::Sint32x2 => 4 * 2,
            VertexFormat::Sint32x3 => 4 * 3,
            VertexFormat::Sint32x4 => 4 * 4,
            VertexFormat::Float64 => 8,
            VertexFormat::Float64x2 => 8 * 2,
            VertexFormat::Float64x3 => 8 * 3,
            VertexFormat::Float64x4 => 8 * 4,
        }
    }
}

/// Contains an array where each entry describes a property of a single vertex.
/// Matches the [`VertexFormats`](VertexFormat).
#[derive(Clone, Debug, EnumVariantMeta)]
pub enum VertexAttributeValues {
    Float32(Vec<f32>),
    Sint32(Vec<i32>),
    Uint32(Vec<u32>),
    Float32x2(Vec<[f32; 2]>),
    Sint32x2(Vec<[i32; 2]>),
    Uint32x2(Vec<[u32; 2]>),
    Float32x3(Vec<[f32; 3]>),
    Sint32x3(Vec<[i32; 3]>),
    Uint32x3(Vec<[u32; 3]>),
    Float32x4(Vec<[f32; 4]>),
    Sint32x4(Vec<[i32; 4]>),
    Uint32x4(Vec<[u32; 4]>),
    Sint16x2(Vec<[i16; 2]>),
    Snorm16x2(Vec<[i16; 2]>),
    Uint16x2(Vec<[u16; 2]>),
    Unorm16x2(Vec<[u16; 2]>),
    Sint16x4(Vec<[i16; 4]>),
    Snorm16x4(Vec<[i16; 4]>),
    Uint16x4(Vec<[u16; 4]>),
    Unorm16x4(Vec<[u16; 4]>),
    Sint8x2(Vec<[i8; 2]>),
    Snorm8x2(Vec<[i8; 2]>),
    Uint8x2(Vec<[u8; 2]>),
    Unorm8x2(Vec<[u8; 2]>),
    Sint8x4(Vec<[i8; 4]>),
    Snorm8x4(Vec<[i8; 4]>),
    Uint8x4(Vec<[u8; 4]>),
    Unorm8x4(Vec<[u8; 4]>),
}

impl VertexAttributeValues {
    /// Returns the number of vertices in this [`VertexAttributeValues`]. For a single
    /// mesh, all of the [`VertexAttributeValues`] must have the same length.
    pub fn len(&self) -> usize {
        match *self {
            VertexAttributeValues::Float32(ref values) => values.len(),
            VertexAttributeValues::Sint32(ref values) => values.len(),
            VertexAttributeValues::Uint32(ref values) => values.len(),
            VertexAttributeValues::Float32x2(ref values) => values.len(),
            VertexAttributeValues::Sint32x2(ref values) => values.len(),
            VertexAttributeValues::Uint32x2(ref values) => values.len(),
            VertexAttributeValues::Float32x3(ref values) => values.len(),
            VertexAttributeValues::Sint32x3(ref values) => values.len(),
            VertexAttributeValues::Uint32x3(ref values) => values.len(),
            VertexAttributeValues::Float32x4(ref values) => values.len(),
            VertexAttributeValues::Sint32x4(ref values) => values.len(),
            VertexAttributeValues::Uint32x4(ref values) => values.len(),
            VertexAttributeValues::Sint16x2(ref values) => values.len(),
            VertexAttributeValues::Snorm16x2(ref values) => values.len(),
            VertexAttributeValues::Uint16x2(ref values) => values.len(),
            VertexAttributeValues::Unorm16x2(ref values) => values.len(),
            VertexAttributeValues::Sint16x4(ref values) => values.len(),
            VertexAttributeValues::Snorm16x4(ref values) => values.len(),
            VertexAttributeValues::Uint16x4(ref values) => values.len(),
            VertexAttributeValues::Unorm16x4(ref values) => values.len(),
            VertexAttributeValues::Sint8x2(ref values) => values.len(),
            VertexAttributeValues::Snorm8x2(ref values) => values.len(),
            VertexAttributeValues::Uint8x2(ref values) => values.len(),
            VertexAttributeValues::Unorm8x2(ref values) => values.len(),
            VertexAttributeValues::Sint8x4(ref values) => values.len(),
            VertexAttributeValues::Snorm8x4(ref values) => values.len(),
            VertexAttributeValues::Uint8x4(ref values) => values.len(),
            VertexAttributeValues::Unorm8x4(ref values) => values.len(),
        }
    }

    /// Returns `true` if there are no vertices in this [`VertexAttributeValues`].
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the values as float triples if possible.
    fn as_float3(&self) -> Option<&[[f32; 3]]> {
        match self {
            VertexAttributeValues::Float32x3(values) => Some(values),
            _ => None,
        }
    }

    // TODO: add vertex format as parameter here and perform type conversions
    /// Flattens the [`VertexAttributeValues`] into a sequence of bytes. This is
    /// useful for serialization and sending to the GPU.
    pub fn get_bytes(&self) -> &[u8] {
        match self {
            VertexAttributeValues::Float32(values) => cast_slice(&values[..]),
            VertexAttributeValues::Sint32(values) => cast_slice(&values[..]),
            VertexAttributeValues::Uint32(values) => cast_slice(&values[..]),
            VertexAttributeValues::Float32x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Sint32x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Uint32x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Float32x3(values) => cast_slice(&values[..]),
            VertexAttributeValues::Sint32x3(values) => cast_slice(&values[..]),
            VertexAttributeValues::Uint32x3(values) => cast_slice(&values[..]),
            VertexAttributeValues::Float32x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Sint32x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Uint32x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Sint16x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Snorm16x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Uint16x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Unorm16x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Sint16x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Snorm16x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Uint16x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Unorm16x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Sint8x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Snorm8x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Uint8x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Unorm8x2(values) => cast_slice(&values[..]),
            VertexAttributeValues::Sint8x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Snorm8x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Uint8x4(values) => cast_slice(&values[..]),
            VertexAttributeValues::Unorm8x4(values) => cast_slice(&values[..]),
        }
    }
}

impl From<&VertexAttributeValues> for VertexFormat {
    fn from(values: &VertexAttributeValues) -> Self {
        match values {
            VertexAttributeValues::Float32(_) => VertexFormat::Float32,
            VertexAttributeValues::Sint32(_) => VertexFormat::Sint32,
            VertexAttributeValues::Uint32(_) => VertexFormat::Uint32,
            VertexAttributeValues::Float32x2(_) => VertexFormat::Float32x2,
            VertexAttributeValues::Sint32x2(_) => VertexFormat::Sint32x2,
            VertexAttributeValues::Uint32x2(_) => VertexFormat::Uint32x2,
            VertexAttributeValues::Float32x3(_) => VertexFormat::Float32x3,
            VertexAttributeValues::Sint32x3(_) => VertexFormat::Sint32x3,
            VertexAttributeValues::Uint32x3(_) => VertexFormat::Uint32x3,
            VertexAttributeValues::Float32x4(_) => VertexFormat::Float32x4,
            VertexAttributeValues::Sint32x4(_) => VertexFormat::Sint32x4,
            VertexAttributeValues::Uint32x4(_) => VertexFormat::Uint32x4,
            VertexAttributeValues::Sint16x2(_) => VertexFormat::Sint16x2,
            VertexAttributeValues::Snorm16x2(_) => VertexFormat::Snorm16x2,
            VertexAttributeValues::Uint16x2(_) => VertexFormat::Uint16x2,
            VertexAttributeValues::Unorm16x2(_) => VertexFormat::Unorm16x2,
            VertexAttributeValues::Sint16x4(_) => VertexFormat::Sint16x4,
            VertexAttributeValues::Snorm16x4(_) => VertexFormat::Snorm16x4,
            VertexAttributeValues::Uint16x4(_) => VertexFormat::Uint16x4,
            VertexAttributeValues::Unorm16x4(_) => VertexFormat::Unorm16x4,
            VertexAttributeValues::Sint8x2(_) => VertexFormat::Sint8x2,
            VertexAttributeValues::Snorm8x2(_) => VertexFormat::Snorm8x2,
            VertexAttributeValues::Uint8x2(_) => VertexFormat::Uint8x2,
            VertexAttributeValues::Unorm8x2(_) => VertexFormat::Unorm8x2,
            VertexAttributeValues::Sint8x4(_) => VertexFormat::Sint8x4,
            VertexAttributeValues::Snorm8x4(_) => VertexFormat::Snorm8x4,
            VertexAttributeValues::Uint8x4(_) => VertexFormat::Uint8x4,
            VertexAttributeValues::Unorm8x4(_) => VertexFormat::Unorm8x4,
        }
    }
}
/// An array of indices into the [`VertexAttributeValues`] for a mesh.
///
/// It describes the order in which the vertex attributes should be joined into faces.
#[derive(Debug, Clone)]
pub enum Indices {
    U16(Vec<u16>),
    U32(Vec<u32>),
}

impl Indices {
    /// Returns an iterator over the indices.
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        match self {
            Indices::U16(vec) => IndicesIter::U16(vec.iter()),
            Indices::U32(vec) => IndicesIter::U32(vec.iter()),
        }
    }

    /// Returns the number of indices.
    pub fn len(&self) -> usize {
        match self {
            Indices::U16(vec) => vec.len(),
            Indices::U32(vec) => vec.len(),
        }
    }

    /// Returns `true` if there are no indices.
    pub fn is_empty(&self) -> bool {
        match self {
            Indices::U16(vec) => vec.is_empty(),
            Indices::U32(vec) => vec.is_empty(),
        }
    }
}

/// An Iterator for the [`Indices`].
enum IndicesIter<'a> {
    U16(std::slice::Iter<'a, u16>),
    U32(std::slice::Iter<'a, u32>),
}

impl Iterator for IndicesIter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IndicesIter::U16(iter) => iter.next().map(|val| *val as usize),
            IndicesIter::U32(iter) => iter.next().map(|val| *val as usize),
        }
    }
}

impl From<&Indices> for IndexFormat {
    fn from(indices: &Indices) -> Self {
        match indices {
            Indices::U16(_) => IndexFormat::Uint16,
            Indices::U32(_) => IndexFormat::Uint32,
        }
    }
}

/// The GPU-representation of a [`Mesh`].
/// Consists of a vertex data buffer and an optional index data buffer.
#[derive(Debug, Clone)]
pub struct GpuMesh {
    /// Contains all attribute data for each vertex.
    pub vertex_buffer: Buffer,
    pub morph_target_image: Option<GpuImage>,
    pub buffer_info: GpuBufferInfo,
    pub primitive_topology: PrimitiveTopology,
    pub layout: MeshVertexBufferLayout,
}

/// The index/vertex buffer info of a [`GpuMesh`].
#[derive(Debug, Clone)]
pub enum GpuBufferInfo {
    Indexed {
        /// Contains all index data of a mesh.
        buffer: Buffer,
        count: u32,
        index_format: IndexFormat,
    },
    NonIndexed {
        vertex_count: u32,
    },
}

impl RenderAsset for Mesh {
    type ExtractedAsset = Mesh;
    type PreparedAsset = GpuMesh;
    type Param = (SRes<RenderDevice>, SRes<RenderQueue>);

    /// Clones the mesh.
    fn extract_asset(&self) -> Self::ExtractedAsset {
        self.clone()
    }

    /// Converts the extracted mesh a into [`GpuMesh`].
    fn prepare_asset(
        mesh: Self::ExtractedAsset,
        param: &mut SystemParamItem<Self::Param>,
    ) -> Result<Self::PreparedAsset, PrepareAssetError<Self::ExtractedAsset>> {
        let morph_target_image = mesh
            .create_morph_target_image()
            .and_then(|image| Image::prepare_asset(image, param).ok());
        let (render_device, _) = &param;
        let vertex_buffer_data = mesh.get_vertex_buffer_data();
        let vertex_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            usage: BufferUsages::VERTEX,
            label: Some("Mesh Vertex Buffer"),
            contents: &vertex_buffer_data,
        });

        let buffer_info = mesh.get_index_buffer_bytes().map_or(
            GpuBufferInfo::NonIndexed {
                vertex_count: mesh.count_vertices() as u32,
            },
            |data| GpuBufferInfo::Indexed {
                buffer: render_device.create_buffer_with_data(&BufferInitDescriptor {
                    usage: BufferUsages::INDEX,
                    contents: data,
                    label: Some("Mesh Index Buffer"),
                }),
                count: mesh.indices().unwrap().len() as u32,
                index_format: mesh.indices().unwrap().into(),
            },
        );

        let mesh_vertex_buffer_layout = mesh.get_mesh_vertex_buffer_layout();

        Ok(GpuMesh {
            vertex_buffer,
            morph_target_image,
            buffer_info,
            primitive_topology: mesh.primitive_topology(),
            layout: mesh_vertex_buffer_layout,
        })
    }
}
