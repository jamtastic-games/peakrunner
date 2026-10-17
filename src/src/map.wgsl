struct U {
    vp: mat4x4<f32>, inv_vp: mat4x4<f32>, eye: vec4<f32>,
    // xyz: direction toward the sun; w: seconds, for cloud drift.
    sun: vec4<f32>,
    fog: vec4<f32>, layers: vec4<f32>, sky0: vec4<f32>, sky1: vec4<f32>,
    // Optional per-map look (peakrunner_core::look); defaults keep the old look.
    sun_color: vec4<f32>,   // rgb, exposure
    amb_sky: vec4<f32>,     // rgb, sky mode (1 = procedural)
    amb_ground: vec4<f32>,  // rgb, sun disc brightness
    zenith: vec4<f32>,      // rgb, cloud cover
    horizon: vec4<f32>,     // rgb, sun angular radius (radians)
    cloud: vec4<f32>,       // rgb, cloud scale
    hfog: vec4<f32>,        // density, base height, falloff, shade-map scale (1/tile metres)
    glow: vec4<f32>,        // bloom: emissive texture layer (-1 none), strength, sun-disc glow
    lights: array<vec4<f32>,4>,
    // The sun's shadow map of the bodies near the camera (scene.rs
    // `ShadowGpu`): world to its clip space, and texel size, depth bias,
    // strength and 1 when there is one.
    light_vp: mat4x4<f32>,
    shadow: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var images: texture_2d_array<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var weights: texture_2d<f32>;
// Baked terrain shade: r = sun visibility, g = ambient occlusion (terrain_shade.py).
@group(0) @binding(4) var shade: texture_2d<f32>;
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_samp: sampler_comparison;
struct In {
    @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>, @location(3) lmuv: vec2<f32>,
    @location(4) layer: vec2<f32>,
}
struct Out {
    @builtin(position) pos: vec4<f32>, @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
    @location(3) lmuv: vec2<f32>, @location(4) @interpolate(flat) layer: vec2<f32>,
}
fn map_noise(p:vec2<f32>)->f32 {
    let i=floor(p);let f=fract(p);let t=f*f*(3.0-2.0*f);
    let corners=vec4<f32>(dot(i,vec2<f32>(127.1,311.7)),dot(i+vec2<f32>(1,0),vec2<f32>(127.1,311.7)),
        dot(i+vec2<f32>(0,1),vec2<f32>(127.1,311.7)),dot(i+vec2<f32>(1,1),vec2<f32>(127.1,311.7)));
    let n=fract(sin(corners)*43758.5453);
    return mix(mix(n.x,n.y,t.x),mix(n.z,n.w,t.x),t.y);
}
fn fbm(p:vec2<f32>)->f32 {
    var s=0.0;var a=0.5;var q=p;
    for (var i=0;i<5;i++) {s+=a*map_noise(q);q=q*2.03+vec2<f32>(17.1,9.2);a*=0.5;}
    return s/0.96875;
}
// Exposure, then a gentle highlight shoulder: identity below 0.8 so the
// existing look is unchanged, rolling off toward 1 instead of clipping.
fn tone(c:vec3<f32>)->vec3<f32> {
    let x=max(c*u.sun_color.w,vec3<f32>(0.0));
    let t=0.8;
    let rolled=vec3<f32>(t)+(1.0-t)*(vec3<f32>(1.0)-exp(-(x-vec3<f32>(t))/(1.0-t)));
    return select(x,rolled,x>vec3<f32>(t));
}
fn layer_mix(uv:vec2<f32>,w:vec4<f32>,l:vec4<i32>)->vec4<f32> {
    return textureSample(images,samp,uv,l.x)*w.x+textureSample(images,samp,uv,l.y)*w.y
        +textureSample(images,samp,uv,l.z)*w.z+textureSample(images,samp,uv,l.w)*w.w;
}
// Point lights (the football's glow): lights[2k] = position, radius;
// lights[2k+1] = colour, strength. A nearly even pool with a soft rim.
fn point_lights(p:vec3<f32>,n:vec3<f32>)->vec3<f32> {
    var sum=vec3<f32>(0.0);
    for (var k=0;k<2;k++) {
        let at=u.lights[k*2];
        if (at.w<=0.0) {continue;}
        let d=at.xyz-p;
        let dist=length(d);
        let reach=1.0-smoothstep(at.w*0.72,at.w,dist);
        let facing=0.35+0.65*max(dot(n,d/max(dist,0.001)),0.0);
        sum+=u.lights[k*2+1].rgb*u.lights[k*2+1].w*reach*facing;
    }
    return sum;
}
// How much sun reaches `p` past the bodies in the shadow map: 1 in the
// open, falling to 0 in a body's shadow (a 3x3 filter of comparisons, faded
// toward the map's edge). Only bodies cast into it; the map's own shadows
// are baked.
fn sunlit(p:vec3<f32>,n:vec3<f32>)->f32 {
    if (u.shadow.w<0.5) {return 1.0;}
    let q=u.light_vp*vec4<f32>(p+n*u.shadow.x*1.5,1.0);
    let c=q.xyz/q.w;
    let uv=vec2<f32>(c.x*0.5+0.5,0.5-c.y*0.5);
    if (any(uv<vec2<f32>(0.0)) || any(uv>vec2<f32>(1.0)) || c.z>1.0) {return 1.0;}
    let texel=1.0/vec2<f32>(textureDimensions(shadow_map));
    var lit=0.0;
    for (var y=-1;y<=1;y++) {
        for (var x=-1;x<=1;x++) {
            lit+=textureSampleCompareLevel(shadow_map,shadow_samp,uv+vec2<f32>(f32(x),f32(y))*texel,c.z-u.shadow.y);
        }
    }
    let edge=smoothstep(0.0,0.08,min(min(uv.x,uv.y),min(1.0-uv.x,1.0-uv.y)));
    return mix(1.0,lit/9.0,u.shadow.z*edge);
}
@vertex fn vs_map(v: In) -> Out {
    var o: Out; o.pos=u.vp*vec4<f32>(v.pos,1.0); o.world=v.pos;
    o.normal=v.normal; o.uv=v.uv; o.lmuv=v.lmuv; o.layer=v.layer; return o;
}
// Render-only prop instances (props.bin): a shared local mesh per shape plus
// per-instance world position, yaw and uniform scale.
struct PropIn {
    @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) material: f32,
    @location(5) at: vec3<f32>, @location(6) yaw_scale: vec2<f32>,
}
// The build kit's Mesh.point rotation: x' = x c + z s, z' = -x s + z c.
fn kit_yaw(p:vec3<f32>,c:f32,s:f32)->vec3<f32> {return vec3<f32>(p.x*c+p.z*s,p.y,-p.x*s+p.z*c);}
@vertex fn vs_prop(v: PropIn) -> Out {
    let c=cos(v.yaw_scale.x);let s=sin(v.yaw_scale.x);
    let world=v.at+kit_yaw(v.pos*v.yaw_scale.y,c,s);
    let n=kit_yaw(v.normal,c,s);
    // The kit's planar texture coordinates: world metres / 4 on the two axes
    // other than the face normal's dominant one (first axis wins ties).
    let a=abs(n);
    var uv=world.xy;
    if (a.x>=a.y && a.x>=a.z) {uv=world.zy;} else if (a.y>=a.z) {uv=world.xz;}
    var o: Out; o.pos=u.vp*vec4<f32>(world,1.0); o.world=world; o.normal=n;
    o.uv=uv/4.0; o.lmuv=vec2<f32>(0.0); o.layer=vec2<f32>(v.material,-1.0); return o;
}
// Lava (water volumes with `lava`, layer -5): dark cooled crust broken by
// glowing seams that drift and pulse, molten pools showing through, lit by
// itself rather than the sun; fog still covers it.
fn lava(p:vec2<f32>,t:f32)->vec3<f32> {
    let drift=vec2<f32>(t*0.08,t*0.05);
    let plates=fbm(p/18.0+drift);
    let heat=fbm(p/7.0-drift*1.4);
    let seams=pow(1.0-abs(2.0*fbm(p/3.2-drift*1.8)-1.0),5.0);
    let fine=pow(1.0-abs(2.0*fbm(p*0.55+drift*2.2)-1.0),4.0)*0.45;
    let pulse=0.82+0.18*sin(t*1.7+plates*8.0);
    let glow=clamp((heat-0.32)*2.1+seams*1.35+fine,0.0,1.0)*pulse;
    let crust=vec3<f32>(0.045,0.02,0.015)*(0.65+0.7*map_noise(p*1.1));
    let hot=mix(vec3<f32>(0.85,0.16,0.02),vec3<f32>(1.0,0.92,0.55),smoothstep(0.55,1.0,glow));
    return mix(crust,hot*(1.35+0.85*glow),glow);
}
// A lava flow lying on the ground (layer -5.5 with a rim): a molten core
// under slow-moving crust plates, cooling to dark crust toward its edge
// (`rim` 0 in the middle, 1 at the edge), where only cracks still glow.
fn flow(p:vec2<f32>,t:f32,rim:f32)->vec3<f32> {
    let drift=vec2<f32>(t*0.05,t*0.03);
    let core=1.0-smoothstep(0.3,0.95,rim);
    let heat=fbm(p/4.5+drift*1.3);
    let plates=pow(1.0-abs(2.0*fbm(p/1.9-drift)-1.0),5.0);
    let cracks=pow(1.0-abs(2.0*fbm(p*0.9+drift*1.6)-1.0),7.0);
    let pulse=0.86+0.14*sin(t*1.3+heat*9.0);
    let glow=clamp((heat-0.38)*2.6*core+plates*(0.25+0.8*core)+cracks*0.55,0.0,1.0)*pulse;
    let crust=vec3<f32>(0.05,0.028,0.02)*(0.6+0.8*map_noise(p*1.3));
    let hot=mix(vec3<f32>(0.7,0.12,0.02),vec3<f32>(1.0,0.6,0.18),smoothstep(0.5,1.0,glow));
    return mix(crust,hot*(1.05+0.6*glow),glow);
}
// Firelight in a hollow (-5.25, Ashveil's skull tower): flames climbing
// the walls, lit by nothing, projected across the wall so it reads on
// vertical faces too. It flickers and rises; brightest low down.
fn fire(p:vec3<f32>,t:f32)->vec3<f32> {
    let q=vec2<f32>(p.x*0.7+p.z*0.7,p.y);
    let tongues=fbm(q*vec2<f32>(1.1,0.55)+vec2<f32>(0.0,-t*1.8))*0.65+fbm(q*vec2<f32>(2.6,1.3)+vec2<f32>(t*0.3,-t*3.1))*0.35;
    let flick=0.75+0.25*sin(t*9.0+p.y*1.3+p.x*0.7)*sin(t*5.3+p.z*0.9);
    let heat=clamp((tongues-0.36)*3.2,0.0,1.0)*flick;
    let hot=mix(vec3<f32>(0.85,0.18,0.03),vec3<f32>(1.0,0.7,0.25),smoothstep(0.55,1.0,heat));
    return mix(vec3<f32>(0.16,0.04,0.02),hot*(1.1+0.5*heat),heat);
}

@fragment fn fs_map(v: Out) -> @location(0) vec4<f32> {
    // Lava (-5), and thin flows and falls drawn as map geometry, cooler:
    // orange rather than white-hot (-5.5; never a lightmap page, which are
    // whole numbers). A flow with a rim (lightmap uv x = 1 + rim) cools to
    // crust at its edge (`flow`).
    if (v.layer.y == -5.25) {
        let dist=distance(u.eye.xyz,v.world);
        let fog=smoothstep(u.fog.w,u.eye.w,dist);
        return vec4<f32>(mix(tone(fire(v.world,u.sun.w)),u.fog.rgb,fog),1.0);
    }
    if (v.layer.y == -5.0 || v.layer.y == -5.5) {
        let dist=distance(u.eye.xyz,v.world);
        let fog=smoothstep(u.fog.w,u.eye.w,dist);
        if (v.layer.y == -5.5 && v.lmuv.x >= 1.0) {
            return vec4<f32>(mix(tone(flow(v.world.xz,u.sun.w,v.lmuv.x-1.0)),u.fog.rgb,fog),1.0);
        }
        let heat=select(vec3<f32>(1.0),vec3<f32>(0.5,0.26,0.13),v.layer.y == -5.5);
        return vec4<f32>(mix(tone(lava(v.world.xz,u.sun.w)*heat),u.fog.rgb,fog),1.0);
    }
    var color=textureSample(images,samp,v.uv,i32(v.layer.x));
    // Water volumes with a colour carry it packed in the otherwise unused
    // lightmap uv: x = r*256 + g, y = b + 1 (0 means no tint).
    if (v.layer.y == -3.0 && v.lmuv.y > 0.5) {
        let r=floor(v.lmuv.x/256.0); let g=v.lmuv.x-r*256.0; let b=v.lmuv.y-1.0;
        let tint=clamp(vec3<f32>(r,g,b)/255.0*2.2,vec3<f32>(0.0),vec3<f32>(1.0));
        color=vec4<f32>(mix(color.rgb,tint,0.6),color.a);
    }
    var n=normalize(v.normal);
    let dist=distance(u.eye.xyz,v.world);
    if (v.layer.y == -2.0) {
        // Original four painted terrain layers, fixed 8m texture scale. The
        // weight map has one texel per 8 m cell: warp the lookup a fraction of
        // a cell and widen the blend with distance, so dominant layers stop
        // flipping cell by cell into a camouflage pattern from the air.
        let cell=1.0/256.0;
        let warp=(vec2<f32>(map_noise(v.world.xz/19.0),map_noise(v.world.xz/19.0+vec2<f32>(7.3,1.9)))-0.5)*cell*0.9;
        let wuv=v.lmuv+warp;
        let spread=cell*mix(0.35,1.1,smoothstep(60.0,500.0,dist));
        var w=textureSample(weights,samp,wuv)*2.0
            +textureSample(weights,samp,wuv+vec2<f32>(spread,0.0))+textureSample(weights,samp,wuv-vec2<f32>(spread,0.0))
            +textureSample(weights,samp,wuv+vec2<f32>(0.0,spread))+textureSample(weights,samp,wuv-vec2<f32>(0.0,spread));
        w/=max(dot(w,vec4<f32>(1.0)),0.0001);
        let l=vec4<i32>(u.layers);
        let base=layer_mix(v.uv,w,l);
        // Close-range detail: the same layers at about a quarter of the tile,
        // rotated so the two scales never line up. It modulates brightness
        // around the base colour and fades out by 90 m.
        let duv=vec2<f32>(v.uv.x*0.8-v.uv.y*0.6,v.uv.x*0.6+v.uv.y*0.8)*4.3;
        let detail=layer_mix(duv,w,l);
        let near=1.0-smoothstep(10.0,90.0,dist);
        let ratio=clamp(dot(detail.rgb,vec3<f32>(0.333))/max(dot(base.rgb,vec3<f32>(0.333)),0.02),0.6,1.5);
        var c=base.rgb*mix(1.0,ratio,near*0.7);
        c*=1.0+(map_noise(v.world.xz*1.7)-0.5)*0.18*near;
        // World-scale variation suppresses visible texture repetition without
        // moving UVs or changing the geometry beneath a skier.
        color=vec4<f32>(c*(0.80+0.35*map_noise(v.world.xz/45.0)+0.10*map_noise(v.world.xz/11.0)),base.a);
    }
    if (color.a<0.4) {discard;}
    let ndl=max(dot(n,u.sun.xyz),0.0);
    // Hemisphere ambient: the defaults average the old flat 0.55 term.
    let hemi=mix(u.amb_ground.rgb,u.amb_sky.rgb,clamp(n.y*0.5+0.5,0.0,1.0));
    // Terrain (-2) and props (-1) take the baked terrain shade where they
    // stand; structures keep their own lightmaps below.
    var sh=vec2<f32>(1.0);
    if (v.layer.y>-2.5 && v.layer.y<-0.5) {
        sh=textureSampleLevel(shade,samp,clamp(v.world.xz*u.hfog.w,vec2<f32>(0.0005),vec2<f32>(0.9995)),0.0).rg;
    }
    // A caster also hides part of the sky, so shadowed ground loses up to a
    // fifth of its ambient as well as the direct sun.
    // Bodies near the camera shadow the sun too (`sunlit`).
    let body=sunlit(v.world,n);
    var light=hemi*sh.y*mix(0.8,1.0,sh.x)+u.sun_color.rgb*(0.45*ndl*sh.x*body);
    if (v.layer.y>=0.0) {
        light=max(vec3<f32>(0.22),textureSampleLevel(images,samp,clamp(v.lmuv,vec2<f32>(0.002),vec2<f32>(0.998)),i32(v.layer.y),0.0).rgb);
        // A lightmap holds sun and sky together: a body's shadow takes back
        // the share a sunlit texel owes the sun (none indoors, where the
        // baked light is dim).
        let sunny=smoothstep(0.42,0.8,dot(light,vec3<f32>(0.333)));
        light*=1.0-(1.0-body)*0.45*sunny;
    }
    if (v.layer.y<=-4.0) {
        let baked=textureSampleLevel(images,samp,clamp(v.lmuv,vec2<f32>(0.002),vec2<f32>(0.998)),i32(-4.0-v.layer.y),0.0).rgb;
        light=max(baked*mix(0.6,1.0,body),hemi+u.sun_color.rgb*(0.7*ndl*body));
    }
    light+=point_lights(v.world,n);
    var rgb=tone(color.rgb*light);
    var fog=smoothstep(u.fog.w,u.eye.w,dist);
    if (u.hfog.x>0.0) {
        // Exponential height fog integrated along the view ray.
        let fall=max(u.hfog.z,0.5);
        let ea=max(u.eye.y-u.hfog.y,0.0)/fall;
        let wa=max(v.world.y-u.hfog.y,0.0)/fall;
        let dh=wa-ea;
        var optical=u.hfog.x*dist*exp(-ea);
        if (abs(dh)>0.001) {optical=u.hfog.x*dist*(exp(-ea)-exp(-wa))/dh;}
        fog=max(fog,clamp(1.0-exp(-optical),0.0,1.0));
    }
    rgb=mix(rgb,u.fog.rgb,fog);
    if (v.layer.y == -3.0) {return vec4<f32>(rgb,0.6);}
    // Bloom mask (glow = 1 - alpha): only the emissive material layer glows,
    // and only its bright texels, so lamp housings and white walls stay put.
    var glow=0.0;
    if (u.glow.x>=0.0 && abs(v.layer.x-u.glow.x)<0.5) {
        glow=u.glow.y*smoothstep(0.45,0.9,dot(color.rgb,vec3<f32>(0.299,0.587,0.114)))*(1.0-fog);
    }
    return vec4<f32>(rgb,1.0-glow);
}
struct SkyOut { @builtin(position) pos:vec4<f32>, @location(0) ndc:vec2<f32> }
@vertex fn vs_map_sky(@builtin(vertex_index) i:u32)->SkyOut {
    let points=array<vec2<f32>,3>(vec2<f32>(-1,-1),vec2<f32>(3,-1),vec2<f32>(-1,3));
    var o:SkyOut;o.pos=vec4<f32>(points[i],0.0,1);o.ndc=points[i];return o;
}
fn cubemap_sky(d:vec3<f32>)->vec3<f32> {
    let a=abs(d);
    var uv=vec2<f32>(0);var layer=u.sky0.x;
    if(a.y>max(a.x,a.z)) {
        uv=vec2<f32>(d.x,-d.z)/a.y;layer=u.sky1.x;
        if(d.y<0.0) {return u.fog.rgb;}
    } else if(a.x>a.z) {
        uv=vec2<f32>(select(-d.z,d.z,d.x>0.0),-d.y)/a.x;
        layer=select(u.sky0.w,u.sky0.y,d.x>0.0);
    } else {
        uv=vec2<f32>(select(d.x,-d.x,d.z>0.0),-d.y)/a.z;
        layer=select(u.sky0.x,u.sky0.z,d.z>0.0);
    }
    return textureSampleLevel(images,samp,clamp(uv*0.5+0.5,vec2<f32>(0.002),vec2<f32>(0.998)),i32(layer),0.0).rgb;
}
@fragment fn fs_map_sky(v:SkyOut)->@location(0) vec4<f32> {
    let wp=u.inv_vp*vec4<f32>(v.ndc,0,1);
    let d=normalize(wp.xyz/wp.w-u.eye.xyz);
    let sun=normalize(u.sun.xyz);
    var c=vec3<f32>(0.0);
    var cover=0.0;
    var size=0.022;
    if (u.amb_sky.w>0.5) {
        // Procedural sky: zenith-to-horizon gradient and a drifting fbm cloud
        // deck projected onto a plane, thinning toward the horizon.
        size=u.horizon.w;
        c=mix(u.horizon.rgb,u.zenith.rgb,pow(max(d.y,0.0),0.55));
        if (d.y>0.0 && u.zenith.w>0.0) {
            let p=d.xz/(d.y+0.12)*u.cloud.w*1.4+vec2<f32>(u.sun.w*0.004,u.sun.w*0.0017);
            cover=smoothstep(1.0-u.zenith.w,1.0-u.zenith.w+0.28,fbm(p))*smoothstep(0.0,0.18,d.y);
            let lit=0.82+0.3*max(dot(d,sun),0.0);
            c=mix(c,u.cloud.rgb*lit,cover*0.92);
        }
    } else {
        c=cubemap_sky(d);
    }
    // A visible sun at the lighting direction: a small soft-edged warm disc,
    // a tight corona and a wide faint glow, dimmed behind cloud. amb_ground.w
    // scales it; 0 hides it.
    let sd=max(dot(d,sun),0.0);
    let clear=1.0-cover*0.9;
    let disc=smoothstep(cos(size*1.6),cos(size*0.7),sd)*clear;
    let corona=pow(sd,900.0)*0.55*clear;
    let glow=pow(sd,64.0)*0.22+pow(sd,8.0)*0.07;
    let warm=u.sun_color.rgb*vec3<f32>(1.0,0.9,0.72);
    let lit=mix(c,warm*1.05,disc*0.92)+warm*(corona+glow)*step(-0.02,d.y);
    c=mix(c,lit,u.amb_ground.w);
    let above=smoothstep(-0.02,0.12,d.y);
    let sun_glow=clamp(disc*0.95+corona*1.5,0.0,1.0)*u.amb_ground.w*above*u.glow.z;
    return vec4<f32>(mix(u.fog.rgb,tone(c),above),1.0-sun_glow);
}
