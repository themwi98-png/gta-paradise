//! Each Burnout shader's texture slot roles: per slot number (the sampler
//! record's +0x8, the shader's own sampler slot) the role hash materials
//! store at +0x4 (see material_samplers). Learnt from every material of stock
//! Burnout and the OTN mod that has its role hashes (53 shaders, no
//! disagreements). A modded map's rebuilt bundles have thousands of
//! materials with the role hashes zeroed and the records reordered: their
//! roles come back from here.

/// (shader id, (slot number, role hash) per slot).
pub static SHADER_SLOT_ROLES: &[(u64, &[(u32, u32)])] = &[
    (0x19EA5B8, &[(0x0, 0x6A640902), (0x1, 0x610F08DB)]), // Specular_1Bit_Singlesided
    (0x6D33BFF, &[(0x0, 0x6A640902)]), // Tunnel_Diffuse_Opaque_Singlesided
    (0x8BF1BC7, &[(0x0, 0x6A640902), (0x1, 0x610F08DB)]), // Specular_Greyscale_Doublesided
    (0xFC76E19, &[(0x0, 0x6A640902), (0x2, 0x387B64D2)]), // VideoWall_Diffuse_Opaque_Singlesided
    (0x1A3BB87F, &[(0x0, 0x6A640902), (0x1, 0x610F08DB), (0x2, 0xD0F65026)]), // MetalSheen_Opaque_Doublesided
    (0x1DD3C67C, &[(0x0, 0x6A640902), (0x1, 0x61D1EACC)]), // Illuminance_Diffuse_1Bit_Doublesided
    (0x21EA2565, &[(0x0, 0x6A640902), (0x1, 0x45E5E34D)]), // Tunnel_Lightmapped_1Bit_Singlesided2
    (0x231E6695, &[(0x0, 0x6A640902), (0x1, 0x45E5E34D)]), // Tunnel_Lightmapped_Opaque_Singlesided2
    (0x265A6366, &[(0x0, 0x6A640902)]), // ShoreLine_Diffuse_Greyscale_Singlesided
    (0x27B8D81B, &[(0x0, 0xC6BA11F3), (0x1, 0xFB9B1028)]), // Tunnel_DriveableSurface_Detailmap_Opaque_Singlesided
    (0x3E99A33C, &[(0x0, 0x6A640902)]), // Diffuse_1Bit_Doublesided
    (0x406B9A36, &[(0x0, 0x6A640902)]), // Diffuse_Greyscale_Singlesided
    (0x48D8D59F, &[(0x0, 0x6A640902)]), // Diffuse_1Bit_Singlesided
    (0x4FBEC35D, &[(0x0, 0xC5957B0C), (0x1, 0xFB9B1028), (0x2, 0xC6BA11F3), (0x3, 0x3E15746F), (0x4, 0xAB5C9D24)]), // Tunnel_Road_Detailmap_Opaque_Singlesided
    (0x508AA1B8, &[(0x0, 0xD0F65026)]), // Glass_Specular_Transparent_Doublesided
    (0x562A888A, &[(0x0, 0x6A640902)]), // Foliage_1Bit_Doublesided
    (0x57C51DC5, &[(0x0, 0x6A640902)]), // Sign_Diffuse_1Bit_Singlesided
    (0x60FA7AB6, &[(0x0, 0x6A640902), (0x1, 0x45E5E34D)]), // Sign_Lightmap_Diffuse_Opaque_Singlesided
    (0x6292B3AE, &[(0x0, 0x6A640902), (0x1, 0x610F08DB)]), // Specular_Opaque_Doublesided
    (0x65990B57, &[(0x0, 0x6A640902), (0x1, 0x610F08DB), (0x2, 0xD0F65026)]), // Building_Opaque_Singlesided
    (0x67802CB1, &[(0x0, 0xDBDB0F80), (0x1, 0x78F295EF), (0x2, 0x39B87765)]), // Terrain_Specular_Opaque_Singlesided
    (0x67C2FB46, &[(0x0, 0x6A640902), (0x1, 0x61D1EACC)]), // Sign_Illuminance_Diffuse_Opaque_Singlesided
    (0x6B77E737, &[(0x0, 0x6A640902)]), // Cruciform_1Bit_Doublesided
    (0x6EDFDA1F, &[(0x0, 0x6A640902)]), // Cruciform_1Bit_Doublesided_Instanced
    (0x7B8E4189, &[(0x0, 0x6A640902), (0x1, 0x610F08DB), (0x2, 0xD0F65026), (0x3, 0xC30E35FE)]), // Building_Night_Opaque_Singlesided
    (0x7BA3C93D, &[(0x0, 0x6A640902), (0x1, 0x45E5E34D)]), // Tunnel_Lightmapped_1Bit_Doublesided2
    (0x819B2EBA, &[(0x0, 0xC6BA11F3), (0x1, 0xFB9B1028)]), // DriveableSurface_DetailMap_Diffuse_Opaque_Singlesided
    (0x829956B4, &[(0x0, 0xC6BA11F3), (0x1, 0xFB9B1028), (0x2, 0xB2F30F83)]), // DriveableSurface_Night_Detailmap_Opaque_Singlesided
    (0x8E32FB9E, &[(0x0, 0xC5957B0C), (0x1, 0xFB9B1028), (0x2, 0xC6BA11F3), (0x3, 0x3E15746F), (0x4, 0xAB5C9D24), (0x5, 0xB2F30F83)]), // Road_Night_Detailmap_Opaque_Singlesided
    (0x8EA27B7B, &[(0x0, 0xC5957B0C), (0x1, 0xFB9B1028), (0x2, 0xC6BA11F3), (0x3, 0x3E15746F), (0x4, 0xAB5C9D24)]), // Road_Detailmap_Opaque_Singlesided
    (0x98EC1271, &[(0x0, 0xDBDB0F80), (0x1, 0x78F295EF), (0x2, 0x39B87765)]), // Terrain_Diffuse_Opaque_Singlesided
    (0x9DC4BFEB, &[(0x0, 0x6A640902), (0x1, 0x45E5E34D)]), // CarStudio_DoNotShipWithThisInTheGame
    (0xA017A749, &[(0x0, 0x6A640902)]), // Sign_Diffuse_Opaque_Singlesided
    (0xAAF108AD, &[(0x0, 0x6A640902), (0x1, 0x610F08DB), (0x2, 0x601EF771)]), // Tint_Specular_Opaque_Singlesided
    (0xABF8FBD2, &[(0x0, 0x6A640902), (0x1, 0x45E5E34D)]), // Tunnel_Lightmapped_Opaque_Doublesided2
    (0xACF81FBA, &[(0x0, 0x6A640902), (0x1, 0x610F08DB)]), // Sign_Specular_Opaque_Singlesided
    (0xB5DBB494, &[(0x0, 0x6A640902)]), // Diffuse_Opaque_Doublesided
    (0xB8D690D4, &[(0x0, 0xC5957B0C), (0x1, 0xFB9B1028), (0x2, 0xC6BA11F3), (0x3, 0x45E5E34D)]), // Tunnel_Lightmapped_Road_Detailmap_Opaque_Singlesided2
    (0xBD9730F7, &[(0x0, 0x6A640902)]), // Tunnel_Diffuse_Opaque_Doublesided
    (0xC39AC237, &[(0x0, 0x6A640902)]), // Diffuse_Opaque_Singlesided
    (0xC4A82EBD, &[(0x0, 0xC6BA11F3), (0x1, 0xFB9B1028)]), // DriveableSurface_Detailmap_Opaque_Singlesided
    (0xC6351DB0, &[(0x0, 0x6A640902), (0x1, 0x61D1EACC)]), // Illuminance_Diffuse_Opaque_Singlesided
    (0xCA27A86B, &[(0x0, 0x6A640902), (0x1, 0x610F08DB)]), // Specular_Greyscale_Singlesided
    (0xCA5B9135, &[(0x0, 0x6A640902), (0x1, 0x610F08DB), (0x2, 0xD0F65026)]), // BuildingGlass_Transparent_Doublesided
    (0xD04FE2B9, &[(0x0, 0x6A640902), (0x1, 0x45E5E34D)]), // Lightmap_Diffuse_Opaque_Singlesided
    (0xD0F2834E, &[(0x0, 0x6A640902), (0x2, 0xD0F65026), (0x4, 0x601EF771)]), // Tint_Building_Opaque_Singlesided
    (0xD21A7342, &[(0x0, 0x6A640902)]), // Diffuse_Greyscale_Doublesided
    (0xDF4B75D0, &[(0x0, 0x6A640902), (0x1, 0x61D1EACC)]), // Illuminance_Diffuse_1Bit_Singlesided
    (0xE3BF57AD, &[(0x0, 0x6A640902), (0x1, 0x610F08DB)]), // Specular_1Bit_Doublesided
    (0xEA70D94B, &[(0x0, 0x6A640902), (0x1, 0x610F08DB)]), // Specular_Opaque_Singlesided
    (0xECDAB073, &[(0x0, 0x72175B2C), (0x1, 0x63958181), (0x2, 0xD0F65026)]), // Water_Specular_Opaque_Singlesided
    (0xF6263C49, &[(0x0, 0x6A640902), (0x1, 0x610F08DB), (0x2, 0x601EF771)]), // Tint_Specular_1Bit_Doublesided
    (0xFC8D6F86, &[(0x0, 0x6A640902), (0x1, 0x61D1EACC), (0x2, 0x291B2F61)]), // FlashingNeon_Diffuse_1Bit_Doublesided
];
