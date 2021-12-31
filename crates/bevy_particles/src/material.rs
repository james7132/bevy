use bevy_app::{App, Plugin};
use bevy_asset::{AddAsset, Handle};
use bevy_ecs::system::{lifetimeless::SRes, SystemParamItem};
use bevy_reflect::TypeUuid;
use bevy_render::{
    render_asset::{PrepareAssetError, RenderAsset, RenderAssetPlugin},
    render_resource::{
        std140::{AsStd140, Std140},
        Buffer, BufferInitDescriptor, BufferUsages,
    },
    renderer::RenderDevice,
    texture::Image,
};

// NOTE: These must match the bit flags in bevy_pbr2/src/render/pbr.frag!
bitflags::bitflags! {
    #[repr(transparent)]
    struct ParticleMaterialFlags: u32 {
        const BASE_COLOR_TEXTURE         = (1 << 0);
        const NONE                       = 0;
        const UNINITIALIZED              = 0xFFFF;
    }
}

#[derive(Default, Debug, Clone, TypeUuid)]
#[uuid = "0078f73d-8715-427e-aa65-dc8e1f485d3d"]
pub struct ParticleMaterial {
    pub base_color_texture: Option<Handle<Image>>,
}

pub(crate) struct ParticleMaterialPlugin;

impl Plugin for ParticleMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(RenderAssetPlugin::<ParticleMaterial>::default())
            .add_asset::<ParticleMaterial>();
    }
}

#[derive(Clone, AsStd140)]
pub(crate) struct ParticleMaterialUniformData {
    pub flags: u32,
}

#[derive(Debug, Clone)]
pub struct GpuParticleMaterial {
    pub buffer: Buffer,
    pub base_color_texture: Option<Handle<Image>>,
}

impl RenderAsset for ParticleMaterial {
    type ExtractedAsset = ParticleMaterial;
    type PreparedAsset = GpuParticleMaterial;
    type Param = SRes<RenderDevice>;

    fn extract_asset(&self) -> Self::ExtractedAsset {
        self.clone()
    }

    fn prepare_asset(
        material: Self::ExtractedAsset,
        render_device: &mut SystemParamItem<Self::Param>,
    ) -> Result<Self::PreparedAsset, PrepareAssetError<Self::ExtractedAsset>> {
        let mut flags = ParticleMaterialFlags::NONE;
        if material.base_color_texture.is_some() {
            flags |= ParticleMaterialFlags::BASE_COLOR_TEXTURE;
        }
        let value = ParticleMaterialUniformData { flags: flags.bits };
        let value_std140 = value.as_std140();

        let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: None,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            contents: value_std140.as_bytes(),
        });
        Ok(GpuParticleMaterial {
            buffer,
            base_color_texture: material.base_color_texture,
        })
    }
}
