use glam::{Mat3, Mat4, Vec3};

use crate::effects::{Effects, ViewAnim};
use crate::sim::{
    MatchState, Team, World, VM_MUZZLE_DISC,
    weapon_reload,
};

/// Additive effect draws per frame. World effects are pushed first, so a
/// storm of particles is what gets trimmed, never flags or jets.
pub const MAX_EMIT_DRAWS: usize = 1200;
/// Alpha-blended smoke/dust/scorch draws per frame; the farthest are dropped.
pub const MAX_SMOKE_DRAWS: usize = 220;
/// Players closer than this get the full-detail articulated model.
pub const PLAYER_DETAIL_RANGE: f32 = 80.0;
/// Players closer than this also get their faces' eyes, brows and lips.
pub const FACE_RANGE: f32 = 30.0;
use crate::terrain::{self, MapId};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MeshId {
    Terrain = 0,
    Cube = 1,
    Sphere = 2,
    Disc = 3,
    Bevel = 4,
    Armor = 5,
    /// Flat top-only disc for ground marks (scorch); the smoke pass gives it
    /// a soft radial edge instead of the sphere-edge fade.
    Decal = 6,
    /// The people's generated meshes (`character_mesh`): a sculpted head
    /// (1 tall, facing -Z), a hemisphere dome (base on y = 0) and a capsule.
    Head = 7,
    Dome = 8,
    Capsule = 9,
}

#[derive(Clone)]
pub struct LitDraw {
    pub mesh: MeshId,
    pub model: Mat4,
    pub color: Vec3,
    pub emit: f32,
    pub mode: f32,
}

#[derive(Clone)]
pub struct EmitDraw {
    pub mesh: MeshId,
    pub model: Mat4,
    pub color: [f32; 4],
}

/// One part of an authored model (`model.rs`), placed in the world.
#[derive(Clone, Copy)]
pub struct ModelDraw {
    /// The model's number (`model::get`) and the part's place in it.
    pub id: u16,
    pub part: u16,
    pub model: Mat4,
    /// The team's color, painted where the texture's team mask says.
    pub team: Vec3,
    /// Glow strength (0: off, 1: as authored).
    pub glow: f32,
    /// Painted `team` all over, not just where the mask says (a round's
    /// `color`).
    pub paint: bool,
    /// Team armor: how much of the plates the artwork leaves bare take
    /// `team` too (0: only the mask; `player_model::TEAM_PLATES`).
    pub plates: f32,
    /// How brightly the painted plates light themselves (0: not at all;
    /// the rabbit's gold, `player_model::Livery`).
    pub shine: f32,
    /// A skinned part's bone palette: its place in `DrawFrame::bones` (in
    /// whole palettes of `model::MAX_BONES`), its `model` then the body's
    /// frame; `RIGID` for a part drawn whole.
    pub skin: u32,
}

/// `ModelDraw::skin` for a part drawn whole.
pub const RIGID: u32 = u32::MAX;

#[derive(Clone)]
pub struct DrawFrame {
    pub eye: Vec3,
    pub sun: Vec3,
    pub fog: Vec3,
    pub view: Mat4,
    pub proj: Mat4,
    pub inv_vp: Mat4,
    pub lit: Vec<LitDraw>,
    pub emit: Vec<EmitDraw>,
    /// Alpha-blended, fogged effects that can darken the scene, back to front.
    pub smoke: Vec<EmitDraw>,
    pub viewmodel: Vec<LitDraw>,
    /// Authored models in the world, and in the first-person view.
    pub models: Vec<ModelDraw>,
    pub vm_models: Vec<ModelDraw>,
    pub vm_proj: Mat4,
    pub dt: f32,
    pub map: MapId,
    pub fog_density: f32,
    pub time: f32,
    /// Deathmatch time of day and weather (the default elsewhere).
    pub conditions: peakrunner_core::conditions::Conditions,
    /// Point lights on the map (`map.wgsl`), two slots of [x, y, z, radius]
    /// then [r, g, b, strength]; a zero radius is off. The football's glow.
    pub lights: [[f32; 4]; 4],
    /// What may cast a sun shadow (`scene::ShadowGpu`, which takes those
    /// near the camera): each body's run of `lit` and of `models`, and
    /// where it is.
    pub casters: Vec<(std::ops::Range<usize>, std::ops::Range<usize>, Vec3)>,
    /// Bone palettes for skinned parts (`ModelDraw::skin`), each
    /// `model::MAX_BONES` matrices: a bone's posed frame times the inverse of
    /// its rest frame, in the body's frame.
    pub bones: Vec<Mat4>,
}

/// Reversed depth: every projection here is glam's [0, 1] `perspective_rh`,
/// and this flips it so the near plane is 1 and the far plane 0. With the
/// 32-bit float depth buffer (compare Greater, cleared to 0) precision then
/// stays near-uniform with distance: about 0.1 mm at a kilometre, where the
/// old mapping (which also squeezed depth into 0.5..1) resolved only ~40 cm,
/// so trim and banners a few centimetres off a wall flickered across a map.
pub fn clip_correct(proj: Mat4) -> Mat4 {
    let reverse = Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0,
    ]);
    reverse * proj
}

/// QA-only: override the render camera from `QA_FLYCAM=x,y,z,yaw_deg,pitch_deg,fov_deg`,
/// a free camera decoupled from the player entity's position/physics, used only by
/// capture tooling (`src/qa_overrides.rs`). Absent the env var, rendering is unaffected.
/// The camera the frame is rendered from, so screen overlays line up with it.
pub(crate) fn view_camera(world: &World) -> (Vec3, Vec3, f32) {
    qa_flycam_override().or_else(|| qa_chase(world)).unwrap_or_else(|| world.camera())
}

/// QA-only: `QA_CHASE=distance,height,angle_deg,fov_deg[,player]` follows the
/// local player's drawn body (or that player index's, as drawn) from that
/// angle around it (0 behind, 90 its right side), for clips of the body in
/// play.
fn qa_chase(world: &World) -> Option<(Vec3, Vec3, f32)> {
    if !crate::qa_overrides::active() { return None; }
    let v: Vec<f32> = std::env::var("QA_CHASE").ok()?.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let (distance, height, angle, fov) = (*v.first()?, *v.get(1)?, *v.get(2)?, *v.get(3)?);
    let who = v.get(4).map_or(world.player_id, |&n| n as usize);
    let p = world.players.get(who).filter(|p| p.alive)?;
    let at = p.pos + world.drawn_offset(who) + if who == world.player_id { world.net_camera_offset } else { Vec3::ZERO };
    let body = at + Vec3::Y * 0.75;
    let (back, right) = (Vec3::new(p.yaw.sin(), 0.0, p.yaw.cos()), Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin()));
    let a = angle.to_radians();
    let eye = body + (back * a.cos() + right * a.sin()) * distance + Vec3::Y * height;
    Some((eye, (body - eye).normalize(), fov))
}

/// The QA fly-camera's position, when `QA_FLYCAM` is set.
pub(crate) fn qa_flycam_eye() -> Option<Vec3> {
    qa_flycam_override().map(|(p, _, _)| p)
}

fn qa_flycam_override() -> Option<(Vec3, Vec3, f32)> {
    if !crate::qa_overrides::active() { return None; }
    let raw = std::env::var("QA_FLYCAM").ok()?;
    let v: Vec<f32> = raw.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    if v.len() != 6 {
        return None;
    }
    let (x, y, z, yaw, pitch, fov) = (v[0], v[1], v[2], v[3].to_radians(), v[4].to_radians(), v[5]);
    let cp = pitch.cos();
    let dir = Vec3::new(-yaw.sin() * cp, pitch.sin(), -yaw.cos() * cp);
    Some((Vec3::new(x, y, z), dir, fov))
}

/// Stateless frame (captures, tests): effects start empty and animation rests.
#[cfg(test)]
pub fn build_frame(world: &World, aspect: f32, dt: f32) -> DrawFrame {
    build_frame_with(world, aspect, dt, &mut Effects::new())
}

pub fn build_frame_with(world: &World, aspect: f32, dt: f32, fx: &mut Effects) -> DrawFrame {
    let (eye, dir, fov) = view_camera(world);
    fx.update(world, eye, dt);
    let view = Mat4::look_to_rh(eye, dir, Vec3::Y);
    let size = crate::terrain::info(world.map).size;
    let reach = peakrunner_core::map_pack::on(world.map).and_then(|p| p.draw_distance()).unwrap_or(0.0);
    let far = (size * 1.05).max(480.0).max(reach);
    // The menu orbit keeps 28 m above everything, so a far nearer plane buys
    // ~30x depth precision for distant shorelines and trim while it spins.
    let near = if world.state == MatchState::Flyby {4.0} else {0.14};
    let proj_gl = Mat4::perspective_rh(fov.to_radians(), aspect.max(0.1), near, far);
    let proj = clip_correct(proj_gl);
    let vp = proj * view;
    let inv_vp = vp.inverse();
    let sun = Vec3::new(-0.35, 0.78, -0.42).normalize();
    let fog = if let Some(pack) = peakrunner_core::map_pack::on(world.map) {Vec3::from_array(pack.fog_color())} else {Vec3::new(0.55, 0.46, 0.38)};

    let mut lit = Vec::new();
    let mut emit = Vec::new();
    let mut smoke = Vec::new();
    let mut models = Vec::new();
    let mut casters = Vec::new();
    let mut bones = Vec::new();

    let imported=peakrunner_core::map_pack::on(world.map).is_some();
    if !imported {lit.push(LitDraw {
        mesh: MeshId::Terrain,
        model: Mat4::IDENTITY,
        color: Vec3::ONE,
        emit: 0.0,
        mode: if world.map != MapId::Valley { 2.0 } else { 1.0 },
    });}

    for p in &world.pillars {
        let y = crate::terrain::height_on(world.map, p.x, p.z);
        lit.push(LitDraw {
            mesh: MeshId::Cube,
            model: Mat4::from_translation(Vec3::new(p.x, y + p.h * 0.5, p.z))
                * Mat4::from_scale(Vec3::new(p.r * 1.6, p.h, p.r * 1.6)),
            color: Vec3::new(0.32, 0.30, 0.28),
            emit: 0.0,
            mode: 0.0,
        });
    }

    if !imported {
        push_base(&mut lit, world, true);
        push_base(&mut lit, world, false);
    }

    // Capture & Hold has no flags in play; the rabbit modes have their one
    // gold flag. A carried flag rides on its carrier's back (player_model).
    let ctf = world.mode.rules() == peakrunner_core::modes::Rules::Ctf;
    for f in world.flags.iter().filter(|f| ctf && f.carrier.is_none()) {
        let color = if f.team == Team::Ember { Vec3::new(0.89, 0.29, 0.20) } else { Vec3::new(0.24, 0.78, 0.88) };
        push_flag(&mut lit, f.pos, color, eye, world.time, f.team.idx() as f32);
    }
    if world.rabbit_mode() && world.rabbit.active && world.rabbit.carrier.is_none() {
        push_flag(&mut lit, world.rabbit.pos, crate::player_model::RABBIT_FLAG, eye, world.time, 2.0);
    }

    for (i, p) in world.players.iter().enumerate() {
        // Your own body is drawn only while tackled (the camera orbits it)
        // or in the third-person view. In first person nothing of it shows,
        // not even your jet's flame (a glow used to sit under you).
        if i == world.player_id && world.state != MatchState::Flyby && p.stun <= 0.0
            && !(world.third_person && p.alive && !world.zoom_active()) && !world.rides_exposed(i) {
            continue;
        }
        // Articulated armor; dead players collapse briefly, then vanish.
        use crate::player_model::Hold;
        let hold = if world.ball.active { if world.ball.carried_by(i) { Hold::Carrier } else { Hold::Free } }
            // A blade shows only while its holder does: one on another floor
            // or round a wall would hang in the air alone (`body_in_sight`).
            else if world.blade_zone_of(i).is_some() {
                if i == world.player_id || body_in_sight(world, eye, i) { Hold::Blade } else { Hold::Free }
            }
            else if world.armed(i) { Hold::Weapon } else { Hold::Free };
        // Bodies are drawn between steps (your own like the camera; offline
        // everyone's). A rider in the open sits in their seat as their
        // vehicle is drawn.
        let shifted;
        let seat = world.riding(i).filter(|_| world.rides_exposed(i)).and_then(|(v, k)| world.vehicles.get(v).map(|c| c.seat_at(k)));
        let p = if let Some(at) = seat {
            let mut moved = p.clone(); moved.pos = at; shifted = moved; &shifted
        } else if world.drawn_offset(i) != glam::Vec3::ZERO {
            let mut moved = p.clone(); moved.pos += world.drawn_offset(i); shifted = moved; &shifted
        } else { p };
        use crate::player_model::Lod;
        // Left out of the snapshot (an enemy cloaked out of sight), or
        // inside a vehicle.
        if p.hidden() || (p.riding > 0 && seat.is_none()) { continue; }
        let d2 = eye.distance_squared(p.pos);
        let lod = if d2 < FACE_RANGE * FACE_RANGE { Lod::Close } else if d2 < PLAYER_DETAIL_RANGE * PLAYER_DETAIL_RANGE { Lod::Near } else { Lod::Far };
        let seen = peakrunner_core::sim::status::visibility(&p.effects);
        let anim = crate::player_model::anim_id(i, p);
        if seen >= 1.0 {
            let (start, first) = (lit.len(), models.len());
            crate::player_model::push_player(&mut lit, &mut emit, Some((&mut models, &mut bones)), p, world.time, lod, fx.players.motion(anim), hold, fx.players.ragdoll(anim), crate::player_model::livery(world, i));
            push_mounts(&mut lit, p, crate::player_model::livery(world, i).accent);
            push_body_extras(&mut lit, &mut emit, p, world.time, crate::player_model::livery(world, i).accent);
            dress_body(&mut lit[start..], p);
            dress_models(&mut models[first..], p);
            casters.push((start..lit.len(), first..models.len(), p.pos));
        } else {
            // Faded (a cloak): the body as a faint shimmer of light.
            let (mut body, mut glow) = (Vec::new(), Vec::new());
            crate::player_model::push_player(&mut body, &mut glow, None, p, world.time, lod, fx.players.motion(anim), hold, fx.players.ragdoll(anim), crate::player_model::livery(world, i));
            push_mounts(&mut body, p, crate::player_model::livery(world, i).accent);
            push_body_extras(&mut body, &mut glow, p, world.time, crate::player_model::livery(world, i).accent);
            dress_body(&mut body, p);
            let shimmer = (seen.max(0.06) + 0.04 * (world.time * 7.0 + i as f32).sin()).clamp(0.02, 1.0);
            for d in body {
                let c = d.color * 0.4 + Vec3::new(0.55, 0.6, 0.75) * 0.6;
                emit.push(EmitDraw { mesh: d.mesh, model: d.model, color: [c.x, c.y, c.z, shimmer] });
            }
        }
    }
    // Football's bodies still down after their players respawned.
    for (p, points, age) in fx.players.corpses() {
        use crate::player_model::{Hold, Lod, Motion};
        let lod = if eye.distance_squared(p.pos) < PLAYER_DETAIL_RANGE * PLAYER_DETAIL_RANGE { Lod::Near } else { Lod::Far };
        let (start, first) = (lit.len(), models.len());
        let motion = Motion { dead_for: Some(age.min(crate::player_model::CORPSE_TIME - 0.05)), ..Motion::default() };
        crate::player_model::push_player(&mut lit, &mut emit, Some((&mut models, &mut bones)), p, world.time, lod, motion, Hold::Free,
            Some(&points), crate::player_model::Livery::team(p.team));
        casters.push((start..lit.len(), first..models.len(), p.pos));
    }
    push_ball(&mut lit, world);

    let registry = peakrunner_core::registry::registry();
    for d in &world.discs {
        // A look built from parts (`registry::Part`), along its flight.
        if let Some(parts) = registry.projectiles.get(d.kind as usize).map(|r| &r.parts).filter(|p| !p.is_empty()) {
            let dir = d.vel.normalize_or(Vec3::NEG_Z);
            let frame = Mat4::from_translation(d.pos) * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::NEG_Z, dir));
            push_parts(&mut lit, &mut emit, frame, parts, side_accent(world, d.team));
            continue;
        }
        let (first_lit, first_emit) = (lit.len(), emit.len());
        // Drawn by its look: the base projectile it resembles.
        match registry.look_kind(d.kind) {
            0 => push_disc_round(&mut lit, &mut emit, d),
            2 => push_grenade_round(&mut lit, &mut emit, d, world.time),
            3 => push_plasma(&mut lit, &mut emit, d, world.time),
            crate::sim::loadout::MORTAR_KIND => push_mortar_round(&mut lit, &mut emit, d, world.time),
            crate::sim::throwables::GRENADE_KIND => push_hand_grenade(&mut lit, &mut emit, d, world.time),
            crate::sim::throwables::MINE_KIND => push_mine(&mut lit, &mut emit, d, world.time, side_accent(world, d.team)),
            crate::sim::rifles::LASER_KIND => push_laser_beam(&mut emit, d, side_accent(world, d.team)),
            crate::sim::rifles::RAIL_KIND => push_rail_slug(&mut emit, d),
            crate::sim::arsenal::PLASMA_KIND => push_plasma_bolt(&mut lit, &mut emit, d, world.time),
            crate::sim::arsenal::BLASTER_KIND => push_blaster_bolt(&mut emit, d),
            _ => push_tracer(&mut lit, &mut emit, d, world.time),
        }
        if let Some(round) = registry.projectiles.get(d.kind as usize) {
            dress_round(&mut lit[first_lit..], &mut emit[first_emit..], d, round);
            push_round_model(&mut models, &mut lit, &mut emit, (first_lit, first_emit), d, round, world.time, side_paint(world, d.team));
            push_tether_line(&mut lit, world, d, round);
            if let Some(trail) = round.trail.filter(|t| t.style == peakrunner_core::registry::TrailStyle::Ribbon) {
                let span = (round.life - d.life).clamp(0.0, trail.life());
                push_ribbon(&mut emit, d, span, trail.width(), trail.color, 0.4);
            }
        }
    }
    for e in &world.deployables { push_deployable(&mut lit, &mut emit, e, world.time); }
    push_point_turrets(&mut lit, &mut emit, &mut models, world);
    push_map_models(&mut models, world);
    if let Some((unit, fits)) = &world.deploy_ghost { push_deploy_ghost(&mut emit, unit, *fits, world.time); }
    for l in &world.loot {
        // A drop riding a falling body is drawn on the body where it's drawn.
        let riding = l.rider_of().and_then(|i| world.players.get(i)).filter(|p| !p.alive && !p.hidden())
            .map(|p| p.pos - Vec3::Y * peakrunner_core::terrain::PLAYER_RADIUS);
        push_loot(&mut lit, l, riding, world.time);
    }
    push_map_features(&mut lit, world);
    for c in &world.vehicles { push_vehicle(&mut lit, &mut emit, c, world); }
    push_beacons(&mut lit, &mut emit, world);
    push_spires(&mut emit, world);
    push_repair_beams(&mut emit, world);
    push_stream_beams(&mut emit, world);

    // Persist actual sampled positions, rather than extrapolating backward:
    // smoke follows the arc and stays behind when a grenade bounces. It is
    // alpha-blended now, so the trail darkens snow and sand instead of vanishing.
    for puff in &world.smoke {
        let t = (puff.age / 0.5).clamp(0.0, 1.0);
        let near_fade = ((puff.pos.distance(eye) - 0.8) / 1.2).clamp(0.0, 1.0);
        smoke.push(EmitDraw {
            mesh: MeshId::Sphere,
            model: Mat4::from_translation(puff.pos)
                * Mat4::from_scale(Vec3::splat(0.08 + t * 0.26)),
            color: [0.60, 0.60, 0.58, 0.34 * (1.0 - t) * near_fade],
        });
    }
    for e in &world.explosions {
        push_blast(&mut emit, e, eye);
    }

    for (d,s) in peakrunner_core::equipment::definitions(world.map).iter().zip(&world.equipment) {
        let color=if s.health<=0. {Vec3::new(0.45,0.08,0.03)}
            else if !s.powered {Vec3::new(0.65,0.34,0.04)} else {Vec3::new(0.16,0.9,0.6)};
        let lamp_height=match d.kind {
            peakrunner_core::equipment::Kind::Repair=>-0.4,
            peakrunner_core::equipment::Kind::Inventory=>3.0,
            peakrunner_core::equipment::Kind::Generator=>3.2,
            peakrunner_core::equipment::Kind::Sensor=>2.0,
            peakrunner_core::equipment::Kind::Turret=>0.7,
        };
        lit.push(LitDraw {mesh:MeshId::Cube,
            model:Mat4::from_translation(d.pos()+Vec3::Y*lamp_height)*Mat4::from_scale(Vec3::new(0.24,0.12,0.24)),
            color,emit:if s.powered {1.0} else {0.1},mode:0.0});
        if d.kind==peakrunner_core::equipment::Kind::Turret {
            if d.weapon==peakrunner_core::equipment::TurretWeapon::Rocket {
                push_rocket_turret(&mut lit,&mut emit,&mut models,d,s,world.time);
            } else {
                push_turret_head(&mut lit,&mut emit,d,s);
            }
        }
        if d.kind==peakrunner_core::equipment::Kind::Sensor {
            push_sensor_head(&mut models,d,s,world.time);
        }
        if d.kind==peakrunner_core::equipment::Kind::Generator && (s.health<=0. || s.offline) {
            push_generator_wreck(&mut emit,&mut smoke,d.pos(),d.radius,world.time,s.health<=0.);
        }
    }
    fx.draw(&mut lit, &mut emit, &mut smoke, eye);
    // Back to front for alpha blending; past the cap the farthest go first.
    let depth = |d: &EmitDraw| d.model.w_axis.truncate().distance_squared(eye);
    smoke.sort_unstable_by(|a, b| depth(b).total_cmp(&depth(a)));
    if smoke.len() > MAX_SMOKE_DRAWS {
        let excess = smoke.len() - MAX_SMOKE_DRAWS;
        smoke.drain(..excess);
    }
    emit.truncate(MAX_EMIT_DRAWS);
    let (viewmodel, vm_models) = if crate::qa_overrides::no_hud() && !crate::qa_overrides::keep_gun() { (Vec::new(), Vec::new()) } else { viewmodel_draws(world, &fx.anim) };
    if !(crate::qa_overrides::no_hud() && !crate::qa_overrides::keep_gun()) { push_first_person_blade(&mut lit, &mut emit, &mut models, world, view, &fx.anim); }

    let fog_density = if imported {0.0012} else {0.0072 * (256.0 / size)};
    // Weather and night pull the fog in: scale by how much nearer it ends.
    let conditions = world.conditions;
    let (fog, fog_density) = {
        let (c, _, far) = conditions.fog(fog.to_array(), 200.0, 450.0);
        (Vec3::from_array(c), fog_density * 450.0 / far.max(1.0))
    };
    DrawFrame {
        eye,
        sun,
        fog,
        view,
        proj,
        inv_vp,
        lit,
        emit,
        smoke,
        viewmodel,
        models,
        vm_models,
        // A separate lens keeps the gun stable when speed widens the world FOV.
        vm_proj: clip_correct(Mat4::perspective_rh(65.0_f32.to_radians(), aspect.max(0.75), 0.05, 8.0)),
        dt,
        map: world.map,
        fog_density,
        time: world.time,
        conditions,
        lights: ball_lights(world),
        casters,
        bones,
    }
}

/// Fly edge-first, like the chambered round, with spin around the disc normal.
/// A wrecked generator smoulders: low smoke curling off its shell and
/// flickering sparks spitting from its sides. While it is being repaired back
/// past half hull the smoke thins and the sparks stop. Everything hugs the
/// outside of the casing (hit radius) and stays low, so it reads in basements;
/// positions are a deterministic function of time.
fn push_generator_wreck(emit:&mut Vec<EmitDraw>,smoke:&mut Vec<EmitDraw>,base:Vec3,radius:f32,time:f32,destroyed:bool) {
    let puffs=if destroyed {10} else {5};
    for k in 0..puffs {
        let phase=(time*0.4+k as f32/puffs as f32).fract();
        let a=k as f32*2.39996+time*0.15;
        let out=radius*0.8+phase*1.4;
        let pos=base+Vec3::new(a.cos()*out,0.6+phase*3.0,a.sin()*out);
        // Alpha-blended: the dark smoke actually darkens the room now.
        smoke.push(EmitDraw {mesh:MeshId::Sphere,
            model:Mat4::from_translation(pos)*Mat4::from_scale(Vec3::splat(0.3+phase*0.8)),
            color:[0.08,0.075,0.07,0.55*(1.-phase)*phase.min(0.15)/0.15]});
    }
    if !destroyed {return;}
    for k in 0..18 {
        let seed=k as f32*12.9898+(time*14.).floor()*78.233;
        let flick=(seed.sin()*43758.547).fract();
        if flick<0.4 {continue;}
        let a=(seed*0.37).sin()*std::f32::consts::TAU;
        // Sparks spit outward and fall; each lives a fraction of a flicker.
        let fall=(time*14.).fract();
        let pos=base+Vec3::new(a.cos()*(radius+0.2+fall*0.6),0.5+flick*3.2-fall*0.8,a.sin()*(radius+0.2+fall*0.6));
        emit.push(EmitDraw {mesh:MeshId::Sphere,
            model:Mat4::from_translation(pos)*Mat4::from_scale(Vec3::splat(0.045+flick*0.04)),
            color:[1.0,0.62+flick*0.3,0.18,1.0]});
    }
}

/// Stateless blast layers from the authoritative record: a white flash, then
/// flame lobes and a shockwave ring. Smoke, debris, sparks and scorch marks
/// come from `effects` so they can outlive the 0.55 s record.
fn push_blast(emit: &mut Vec<EmitDraw>, e: &crate::sim::Explosion, eye: Vec3) {
    // Drawn by its projectile's look.
    let e = &crate::sim::Explosion { kind: peakrunner_core::registry::registry().look_kind(e.kind), ..e.clone() };
    let t = (e.age / 0.42).clamp(0.0, 1.0);
    let a = (1.0 - t) * 0.85;
    let flash = (1.0 - e.age / 0.08).max(0.0);
    if flash > 0.0 {
        let mortar = e.kind == crate::sim::loadout::MORTAR_KIND;
        let r = e.max_r * if e.kind == 4 { 0.45 } else if mortar { 0.18 } else { 0.28 } * (1.25 - flash * 0.25);
        emit.push(EmitDraw { mesh: MeshId::Sphere,
            model: Mat4::from_translation(e.pos) * Mat4::from_scale(Vec3::splat(r)),
            color: if mortar { [0.7, 1.0, 0.55, flash * 0.85] } else { [1.0, 0.97, 0.9, flash * 0.9] } });
    }
    // Far away only the flash reads anyway; skip the multi-draw layers.
    if e.pos.distance(eye) > crate::effects::DETAIL_RANGE { return; }
    let ring = |emit: &mut Vec<EmitDraw>, at: Vec3, r: f32, color: [f32; 4]| emit.push(EmitDraw {
        mesh: MeshId::Disc,
        model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::new(r.max(0.35), 0.12, r.max(0.35))),
        color });
    match e.kind {
        0 => {
            flame_burst(emit, e.pos, e.age, BURST_BLUE);
            ring(emit, e.pos + Vec3::Y * 0.2, 0.35 + e.max_r * t, [0.12, 0.55, 1.0, a * 0.18]);
        }
        2 | 6 | 7 => {
            flame_burst(emit, e.pos, e.age, BURST_FIRE);
            shock_ring(emit, e.pos + Vec3::Y * 0.3, 0.6 + e.max_r * 1.1 * t, 0.18, [1.0, 0.62, 0.3, a * 0.5], t);
        }
        crate::sim::loadout::MORTAR_KIND => {
            // The mortar's green burst: two lobed flames and a wide ring.
            flame_burst(emit, e.pos, e.age, BURST_GREEN);
            flame_burst(emit, e.pos + Vec3::Y * (0.8 + t * 1.5), e.age, BURST_GREEN);
            shock_ring(emit, e.pos + Vec3::Y * 0.3, 0.8 + e.max_r * 1.1 * t, 0.24, [0.45, 1.0, 0.3, a * 0.55], t);
        }
        3 => {
            let r = e.max_r * (0.2 + t * 0.7);
            emit.push(EmitDraw { mesh: MeshId::Sphere,
                model: Mat4::from_translation(e.pos) * Mat4::from_scale(Vec3::splat(r)),
                color: [0.35, 1.0, 0.12, -a * 0.7] });
            ring(emit, e.pos, 0.3 + e.max_r * 0.9 * t, [0.35, 1.0, 0.2, a * 0.2]);
        }
        crate::sim::arsenal::PLASMA_KIND => {
            // The plasma gun's burst: an orange bloom and a quick ring.
            let [cr, cg, cb] = PLASMA_GLOW.to_array();
            let r = e.max_r * (0.25 + t * 0.7);
            emit.push(EmitDraw { mesh: MeshId::Sphere,
                model: Mat4::from_translation(e.pos) * Mat4::from_scale(Vec3::splat(r)),
                color: [cr, cg, cb, -a * 0.75] });
            ring(emit, e.pos + Vec3::Y * 0.1, 0.3 + e.max_r * 0.9 * t, [cr, cg, cb, a * 0.25]);
        }
        4 => {
            // Generator: rolling flame tongues around a hot core and a wide
            // ground shockwave, instead of one orange bubble. Cosmetic only.
            for (dx, dz) in [(0.0, 0.0), (1.6, 0.4), (-1.3, 1.1), (0.4, -1.5)] {
                flame_burst(emit, e.pos + Vec3::new(dx, 0.6 + t * 2.5, dz), e.age, BURST_FIRE);
            }
            // A short-lived hot core, then the flames carry the blast.
            let core = (1.0 - e.age / 0.2).max(0.0);
            if core > 0.0 {
                emit.push(EmitDraw { mesh: MeshId::Sphere,
                    model: Mat4::from_translation(e.pos) * Mat4::from_scale(Vec3::splat(e.max_r * (0.12 + 0.12 * (1.0 - core)))),
                    color: [1.0, 0.8, 0.5, core * 0.8] });
            }
            shock_ring(emit, e.pos - Vec3::Y * 1.6, 1.0 + e.max_r * 1.5 * t, 0.3, [1.0, 0.6, 0.28, a * 0.55], t);
        }
        _ => {
            let r = e.max_r * (0.2 + t * 0.9);
            emit.push(EmitDraw { mesh: MeshId::Sphere,
                model: Mat4::from_translation(e.pos) * Mat4::from_scale(Vec3::splat(r)),
                color: [1.0, 0.55, 0.18, a] });
        }
    }
}

/// Expanding shockwave drawn as a ring of soft segments, not a filled disc.
fn shock_ring(emit: &mut Vec<EmitDraw>, center: Vec3, radius: f32, thickness: f32, color: [f32; 4], t: f32) {
    if t > 0.7 || color[3] <= 0.01 { return; }
    const SEGMENTS: usize = 16;
    let arc = std::f32::consts::TAU * radius / SEGMENTS as f32;
    for k in 0..SEGMENTS {
        let a = k as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let at = center + Vec3::new(a.cos(), 0.0, a.sin()) * radius;
        emit.push(EmitDraw { mesh: MeshId::Sphere,
            model: Mat4::from_translation(at) * Mat4::from_rotation_y(-a)
                * Mat4::from_scale(Vec3::new(thickness, thickness * 0.6, arc * 0.6)),
            color: [color[0], color[1], color[2], -color[3]] });
    }
}

/// Disc in flight: a bright rim around a paler glowing hub, two rim studs that
/// sweep round so the spin reads, and a longer ribbon wake. The hub stays
/// thinner than the rim and self-lit: a dark, thicker hub read as a black
/// bar whenever a disc flew at the player edge-on.
fn push_disc_round(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc) {
    let model = spinning_disc(d.pos, d.vel, d.spin, 0.42, 0.06);
    lit.push(LitDraw { mesh: MeshId::Disc, model, color: Vec3::new(0.10, 0.74, 1.0), emit: 1.1, mode: 0.0 });
    lit.push(LitDraw { mesh: MeshId::Disc, model: model * Mat4::from_scale(Vec3::new(0.6, 1.05, 0.6)),
        color: Vec3::new(0.40, 0.78, 1.0), emit: 0.6, mode: 0.0 });
    for k in [0.0, std::f32::consts::PI] {
        lit.push(LitDraw { mesh: MeshId::Cube,
            model: model * Mat4::from_rotation_y(k) * Mat4::from_translation(Vec3::new(0.84, 0.0, 0.0))
                * Mat4::from_scale(Vec3::new(0.18, 1.7, 0.18)),
            color: Vec3::new(0.85, 0.97, 1.0), emit: 1.3, mode: 0.0 });
    }
    // Limit the wake to the flight age so a new shot never trails its launch.
    let trail_time = (5.0 - d.life).clamp(0.0, 0.14).min(9.0 / d.vel.length().max(1.0));
    push_ribbon(emit, d, trail_time, 0.252, [0.45, 0.8, 1.0], 0.3);
}

/// A glowing ribbon behind a round over the last `span` seconds of its
/// flight: `width` metres across at the round, flat, tapering and fading
/// behind it. Each segment reaches its neighbours' centres (the first
/// from the round itself) at half the glow, so the band reads even, not
/// dashed.
fn push_ribbon(emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc, span: f32, width: f32, color: [f32; 3], alpha: f32) {
    if span <= 0.0 { return; }
    let forward = d.vel.normalize_or_zero();
    let mut right = forward.cross(Vec3::Y).normalize_or_zero();
    if right.length_squared() < 0.01 { right = Vec3::X; }
    let up = right.cross(forward);
    const SEGMENTS: usize = 12;
    for k in 1..=SEGMENTS {
        let t = k as f32 / SEGMENTS as f32;
        let half = width * 0.5 * (1.0 - t * 0.75);
        let center = d.pos - d.vel * (span * k as f32 / SEGMENTS as f32);
        let length = d.vel.length() * span / SEGMENTS as f32;
        emit.push(EmitDraw { mesh: MeshId::Sphere,
            model: Mat4::from_cols((right * half).extend(0.0), (up * half / 3.6).extend(0.0),
                (-forward * length).extend(0.0), center.extend(1.0)),
            color: [color[0], color[1], color[2], alpha * 0.5 * (1.0 - t * 0.85)] });
    }
}

/// Stubby grenade round whose fuse light blinks faster as detonation nears.
fn push_grenade_round(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc, time: f32) {
    let spin = Mat4::from_translation(d.pos) * Mat4::from_rotation_y(d.spin) * Mat4::from_rotation_x(d.spin * 0.6);
    lit.push(LitDraw { mesh: MeshId::Sphere, model: spin * Mat4::from_scale(Vec3::new(0.17, 0.15, 0.17)),
        color: Vec3::new(0.26, 0.31, 0.15), emit: 0.05, mode: 0.0 });
    lit.push(LitDraw { mesh: MeshId::Cube, model: spin * Mat4::from_scale(Vec3::new(0.24, 0.045, 0.24)),
        color: Vec3::new(0.95, 0.62, 0.12), emit: 0.35, mode: 0.0 });
    let rate = 3.0 + 14.0 * (1.0 - (d.life / 2.0).clamp(0.0, 1.0));
    let on = (time * rate).fract() < 0.5 || d.life < 0.25;
    let fuse = d.pos + Vec3::Y * 0.15;
    lit.push(LitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(fuse) * Mat4::from_scale(Vec3::splat(0.045)),
        color: Vec3::new(1.0, 0.28, 0.08), emit: if on { 1.6 } else { 0.08 }, mode: 0.0 });
    if on {
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(fuse) * Mat4::from_scale(Vec3::splat(0.11)),
            color: [1.0, 0.32, 0.08, 0.55] });
    }
}

/// Hand grenade: a stubby ribbed canister with a safety lever, tumbling in
/// flight; its fuse light blinks faster as the two seconds run down.
fn push_hand_grenade(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc, time: f32) {
    let rest = crate::sim::throwables::resting(d);
    let body = Mat4::from_translation(d.pos)
        * if rest { Mat4::from_rotation_z(1.2) } else { Mat4::from_rotation_y(d.spin * 0.4) * Mat4::from_rotation_x(d.spin * 0.3) };
    let olive = Vec3::new(0.24, 0.29, 0.16);
    lit.push(LitDraw { mesh: MeshId::Sphere, model: body * Mat4::from_scale(Vec3::new(0.11, 0.14, 0.11)), color: olive, emit: 0.0, mode: 0.0 });
    for k in [-1.0, 0.0, 1.0] {
        lit.push(LitDraw { mesh: MeshId::Disc, model: body * Mat4::from_translation(Vec3::Y * (k * 0.055))
            * Mat4::from_scale(Vec3::new(0.118, 0.012, 0.118)), color: olive * 0.6, emit: 0.0, mode: 0.0 });
    }
    lit.push(LitDraw { mesh: MeshId::Disc, model: body * Mat4::from_translation(Vec3::Y * 0.15)
        * Mat4::from_scale(Vec3::new(0.05, 0.04, 0.05)), color: Vec3::new(0.45, 0.47, 0.5), emit: 0.0, mode: 0.0 });
    lit.push(LitDraw { mesh: MeshId::Cube, model: body * Mat4::from_translation(Vec3::new(0.06, 0.08, 0.0)) * Mat4::from_rotation_z(-0.25)
        * Mat4::from_scale(Vec3::new(0.018, 0.14, 0.035)), color: Vec3::new(0.45, 0.47, 0.5), emit: 0.0, mode: 0.0 });
    let left = (d.life / peakrunner_core::defs::throws().grenade_fuse).clamp(0.0, 1.0);
    let on = (time * (3.0 + 16.0 * (1.0 - left))).fract() < 0.5 || d.life < 0.25;
    let fuse = body.transform_point3(Vec3::Y * 0.19);
    lit.push(LitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(fuse) * Mat4::from_scale(Vec3::splat(0.03)),
        color: Vec3::new(1.0, 0.3, 0.08), emit: if on { 1.6 } else { 0.08 }, mode: 0.0 });
    if on {
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(fuse) * Mat4::from_scale(Vec3::splat(0.08)),
            color: [1.0, 0.32, 0.08, 0.5] });
    }
}

/// Mine: a low dark puck with three trigger prongs and a team light that
/// stays dim until the mine has come to rest and armed, then pulses slowly.
fn push_mine(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc, time: f32, accent: Vec3) {
    use crate::sim::throwables::{armed, resting};
    let body = Mat4::from_translation(d.pos - if resting(d) { Vec3::Y * 0.07 } else { Vec3::ZERO })
        * Mat4::from_rotation_y(if resting(d) { d.pos.x * 1.7 } else { d.spin * 0.5 })
        * if resting(d) { Mat4::IDENTITY } else { Mat4::from_rotation_x(d.spin * 0.35) };
    let steel = Vec3::new(0.2, 0.21, 0.22);
    lit.push(LitDraw { mesh: MeshId::Disc, model: body * Mat4::from_scale(Vec3::new(0.24, 0.07, 0.24)), color: steel, emit: 0.0, mode: 0.0 });
    lit.push(LitDraw { mesh: MeshId::Disc, model: body * Mat4::from_translation(Vec3::Y * 0.045)
        * Mat4::from_scale(Vec3::new(0.16, 0.03, 0.16)), color: steel * 0.55, emit: 0.0, mode: 0.0 });
    for k in 0..3 {
        let a = k as f32 * std::f32::consts::TAU / 3.0;
        lit.push(LitDraw { mesh: MeshId::Cube, model: body * Mat4::from_translation(Vec3::new(a.cos() * 0.09, 0.09, a.sin() * 0.09))
            * Mat4::from_scale(Vec3::new(0.014, 0.06, 0.014)), color: Vec3::new(0.5, 0.52, 0.55), emit: 0.0, mode: 0.0 });
    }
    let team = accent;
    let live = armed(d);
    let glow = if live { 0.4 + 0.9 * (0.5 + 0.5 * (time * 2.5 + d.pos.z).sin()) } else { 0.12 };
    lit.push(LitDraw { mesh: MeshId::Sphere, model: body * Mat4::from_translation(Vec3::Y * 0.07)
        * Mat4::from_scale(Vec3::new(0.04, 0.025, 0.04)), color: team, emit: glow, mode: 0.0 });
    if live && glow > 1.0 {
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: body * Mat4::from_translation(Vec3::Y * 0.07) * Mat4::from_scale(Vec3::splat(0.07)),
            color: [team.x, team.y, team.z, 0.35] });
    }
}

/// Mortar shell: a heavy dark finned shell with a glowing green band and a
/// green glow round it, so a long lob reads across the map.
fn push_mortar_round(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc, time: f32) {
    let facing = d.vel.normalize_or(Vec3::Y);
    let body = Mat4::from_translation(d.pos) * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::Y, facing));
    let green = Vec3::new(0.3, 1.0, 0.2);
    lit.push(LitDraw { mesh: MeshId::Sphere, model: body * Mat4::from_scale(Vec3::new(0.2, 0.34, 0.2)),
        color: Vec3::new(0.12, 0.2, 0.12), emit: 0.0, mode: 0.0 });
    for fin in [Vec3::new(0.3, 0.12, 0.04), Vec3::new(0.04, 0.12, 0.3)] {
        lit.push(LitDraw { mesh: MeshId::Cube, model: body * Mat4::from_translation(Vec3::new(0.0, -0.22, 0.0))
            * Mat4::from_scale(fin), color: Vec3::new(0.26, 0.32, 0.26), emit: 0.0, mode: 0.0 });
    }
    let pulse = 0.5 + 0.5 * (time * 9.0).sin();
    lit.push(LitDraw { mesh: MeshId::Cube, model: body * Mat4::from_translation(Vec3::new(0.0, 0.05, 0.0))
        * Mat4::from_scale(Vec3::new(0.25, 0.05, 0.25)), color: green, emit: 1.2 + pulse * 0.6, mode: 0.0 });
    emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(d.pos) * Mat4::from_scale(Vec3::splat(0.3 + pulse * 0.06)),
        color: [0.3, 1.0, 0.2, 0.3] });
}

/// The hologram of a unit being lined up: the real model as translucent
/// light, in the team's colour where it fits and gray where it can't go.
fn push_deploy_ghost(emit: &mut Vec<EmitDraw>, unit: &crate::sim::deploy::Deployable, fits: bool, time: f32) {
    let (mut lit, mut glow) = (Vec::new(), Vec::new());
    push_deployable(&mut lit, &mut glow, unit, time);
    let tint = if fits { team_accent(unit.team) } else { Vec3::splat(0.55) };
    let shimmer = 0.3 + 0.06 * (time * 6.0).sin();
    for d in lit {
        let c = d.color * 0.35 + tint * 0.65;
        emit.push(EmitDraw { mesh: d.mesh, model: d.model, color: [c.x, c.y, c.z, shimmer] });
    }
    for d in glow {
        emit.push(EmitDraw { mesh: d.mesh, model: d.model, color: [tint.x, tint.y, tint.z, d.color[3].abs().min(0.3)] });
    }
}

/// The deployer that replaces the weapon while you line up a pack: a
/// handheld projector with a small hologram of the unit spinning above it.
fn deployer_draws(frame: Mat4, kind: crate::sim::deploy::DeployKind, team: Team, fits: bool, time: f32) -> Vec<LitDraw> {
    let steel = Vec3::new(0.3, 0.33, 0.36);
    let dark = Vec3::new(0.07, 0.08, 0.09);
    let accent = team_accent(team);
    let lamp = if fits { accent } else { Vec3::splat(0.55) };
    let mut draws = Vec::with_capacity(24);
    let mut put = |mesh, at: Vec3, size: Vec3, color: Vec3, emit: f32| draws.push(LitDraw { mesh,
        model: frame * Mat4::from_translation(at) * Mat4::from_scale(size), color, emit, mode: 0.0 });
    put(MeshId::Bevel, Vec3::new(0.0, -0.05, 0.05), Vec3::new(0.2, 0.12, 0.34), steel, 0.0);
    put(MeshId::Bevel, Vec3::new(0.0, -0.16, 0.14), Vec3::new(0.08, 0.16, 0.1), dark, 0.0);
    put(MeshId::Disc, Vec3::new(0.0, 0.02, -0.06), Vec3::new(0.09, 0.02, 0.09), dark, 0.0);
    put(MeshId::Disc, Vec3::new(0.0, 0.035, -0.06), Vec3::new(0.07, 0.01, 0.07), lamp, 0.9);
    put(MeshId::Cube, Vec3::new(0.101, -0.05, 0.05), Vec3::new(0.004, 0.03, 0.26), accent, 0.4);
    // A small hologram of the unit above the emitter.
    let unit = crate::sim::deploy::Deployable { owner: 0, kind, team, pos: Vec3::ZERO, yaw: time * 0.8,
        health: crate::sim::deploy::max_health(kind), cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 };
    let (mut lit, mut glow) = (Vec::new(), Vec::new());
    push_deployable(&mut lit, &mut glow, &unit, time);
    let scale = match kind.behavior() { peakrunner_core::registry::Behavior::Wall | peakrunner_core::registry::Behavior::Field
        | peakrunner_core::registry::Behavior::Pad | peakrunner_core::registry::Behavior::Gate => 0.035, _ => 0.07 };
    let mini = frame * Mat4::from_translation(Vec3::new(0.0, 0.06, -0.06)) * Mat4::from_scale(Vec3::splat(scale));
    for d in lit {
        draws.push(LitDraw { mesh: d.mesh, model: mini * d.model, color: d.color * 0.4 + lamp * 0.6, emit: 0.45, mode: 0.0 });
    }
    draws
}

/// Deployables, by their look (`placed::look`): a squat tripod turret, a
/// solid wall panel, a glowing force field in its team's colour between
/// two posts, an ammo station crate, an inventory or command kiosk, a
/// pylon, sensor or camera post, a pad, a gate, or a beacon.
fn push_deployable(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, e: &crate::sim::deploy::Deployable, time: f32) {
    use peakrunner_core::registry::DeployLook as Behavior;
    // A wall or field drawn to its entry's size (the base panel's by default).
    let panel = crate::sim::deploy::half_extents(e.kind).unwrap_or(crate::sim::deploy::PANEL_HALF);
    let team = team_accent(e.team);
    let steel = Vec3::new(0.32, 0.36, 0.40);
    let dark = Vec3::new(0.08, 0.09, 0.11);
    let hurt = (e.health / crate::sim::deploy::max_health(e.kind)).clamp(0.0, 1.0);
    let base = Mat4::from_translation(e.pos) * Mat4::from_rotation_y(e.yaw);
    // A look built from parts (`registry::Part`).
    let parts = &e.kind.def().parts;
    if !parts.is_empty() { push_parts(lit, emit, base, parts, team_accent(e.team)); return; }
    match crate::sim::placed::look(e.kind) {
        Behavior::Turret => {
            for k in 0..3 {
                let a = k as f32 / 3.0 * std::f32::consts::TAU;
                let foot = Vec3::new(a.sin() * 0.75, 0.0, a.cos() * 0.75);
                let top = Vec3::new(0.0, 0.8, 0.0);
                let mid = (foot + top) * 0.5;
                let dir = (top - foot).normalize();
                lit.push(LitDraw { mesh: MeshId::Cube, model: Mat4::from_translation(e.pos + mid)
                    * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::Y, dir)) * Mat4::from_scale(Vec3::new(0.08, (top - foot).length(), 0.08)),
                    color: dark, emit: 0.0, mode: 0.0 });
            }
            let aim = e.aim.normalize_or(Vec3::NEG_Z);
            let head = Mat4::from_translation(e.pos + Vec3::Y * 0.95)
                * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::NEG_Z, aim));
            lit.push(LitDraw { mesh: MeshId::Bevel, model: head * Mat4::from_scale(Vec3::new(0.5, 0.36, 0.6)), color: steel, emit: 0.0, mode: 0.0 });
            lit.push(LitDraw { mesh: MeshId::Cube, model: head * Mat4::from_translation(Vec3::new(0.0, 0.0, -0.55))
                * Mat4::from_scale(Vec3::new(0.09, 0.09, 0.6)), color: dark, emit: 0.0, mode: 0.0 });
            lit.push(LitDraw { mesh: MeshId::Cube, model: head * Mat4::from_translation(Vec3::new(0.0, 0.19, 0.0))
                * Mat4::from_scale(Vec3::new(0.52, 0.04, 0.4)), color: team, emit: 0.3, mode: 0.0 });
        }
        Behavior::Wall => {
            // A plated panel: pale armor plate in a dark frame, dulling as it's damaged.
            let plate = Vec3::new(0.62, 0.64, 0.66) * (0.55 + 0.45 * hurt);
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * panel.y)
                * Mat4::from_scale(panel * 2.0), color: plate, emit: 0.0, mode: 0.0 });
            for s in [-1.0f32, 1.0] {
                lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::new(s * panel.x, panel.y, 0.0))
                    * Mat4::from_scale(Vec3::new(0.16, panel.y * 2.0 + 0.1, panel.z * 2.4)), color: dark, emit: 0.0, mode: 0.0 });
            }
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * panel.y.min(1.0))
                * Mat4::from_scale(Vec3::new(panel.x * 2.0, 0.18, panel.z * 2.1)), color: team, emit: 0.15, mode: 0.0 });
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * (panel.y * 2.0 - 0.25).max(panel.y))
                * Mat4::from_scale(Vec3::new(panel.x * 2.02, 0.12, panel.z * 2.1)), color: team, emit: 0.2, mode: 0.0 });
        }
        Behavior::Ammo => {
            use crate::sim::deploy::AMMO_HALF;
            // A supply crate: dark body, team band, lid, and glowing cells.
            lit.push(LitDraw { mesh: MeshId::Bevel, model: base * Mat4::from_translation(Vec3::Y * AMMO_HALF.y)
                * Mat4::from_scale(AMMO_HALF * 2.0), color: Vec3::new(0.24, 0.27, 0.22) * (0.6 + 0.4 * hurt), emit: 0.0, mode: 0.0 });
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * AMMO_HALF.y * 1.2)
                * Mat4::from_scale(Vec3::new(AMMO_HALF.x * 2.04, 0.1, AMMO_HALF.z * 2.04)), color: team, emit: 0.2, mode: 0.0 });
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * (AMMO_HALF.y * 2.0 + 0.03))
                * Mat4::from_scale(Vec3::new(AMMO_HALF.x * 1.8, 0.06, AMMO_HALF.z * 1.8)), color: steel, emit: 0.0, mode: 0.0 });
            let glow = 0.8 + 0.4 * (time * 2.5 + e.pos.z).sin().abs();
            for k in [-1.0f32, 0.0, 1.0] {
                lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::new(k * 0.28, AMMO_HALF.y * 0.7, -AMMO_HALF.z - 0.01))
                    * Mat4::from_scale(Vec3::new(0.14, 0.24, 0.03)), color: Vec3::new(1.0, 0.78, 0.25), emit: glow, mode: 0.0 });
            }
        }
        Behavior::Inventory | Behavior::Command => {
            use crate::sim::deploy::INVENTORY_HALF as H;
            // A command kiosk's screens glow green, with an aerial on top.
            let team = if crate::sim::placed::look(e.kind) == Behavior::Command {
                lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::new(H.x * 0.6, H.y * 2.0 + 0.5, 0.0))
                    * Mat4::from_scale(Vec3::new(0.03, 0.8, 0.03)), color: dark, emit: 0.0, mode: 0.0 });
                Vec3::new(0.3, 1.0, 0.45)
            } else { team };
            // A kiosk: a dark plinth, a pale body with team stripes down its
            // sides, a team-lit screen on each face and a light bar on top.
            let pale = Vec3::new(0.58, 0.61, 0.65) * (0.6 + 0.4 * hurt);
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * 0.08)
                * Mat4::from_scale(Vec3::new(H.x * 2.3, 0.16, H.z * 2.4)), color: dark, emit: 0.0, mode: 0.0 });
            let body = base * Mat4::from_translation(Vec3::Y * (H.y + 0.08));
            lit.push(LitDraw { mesh: MeshId::Bevel, model: body * Mat4::from_scale(Vec3::new(H.x * 2.0, H.y * 1.84, H.z * 1.6)),
                color: pale, emit: 0.0, mode: 0.0 });
            for side in [-1.0f32, 1.0] {
                lit.push(LitDraw { mesh: MeshId::Cube, model: body * Mat4::from_translation(Vec3::new(side * H.x * 1.01, 0.0, 0.0))
                    * Mat4::from_scale(Vec3::new(0.03, H.y * 1.5, H.z * 0.5)), color: team, emit: 0.35, mode: 0.0 });
            }
            let glow = 0.55 + 0.25 * (time * 1.7 + e.pos.x).sin().abs();
            // The screen is on its front (+Z, toward whoever placed it).
            for face in [1.0f32] {
                lit.push(LitDraw { mesh: MeshId::Cube, model: body * Mat4::from_translation(Vec3::new(0.0, 0.2, face * H.z * 0.82))
                    * Mat4::from_scale(Vec3::new(H.x * 1.5, H.y * 0.6, 0.04)), color: team, emit: glow, mode: 0.0 });
                for k in [-1.0f32, 1.0] {
                    lit.push(LitDraw { mesh: MeshId::Cube, model: body * Mat4::from_translation(Vec3::new(k * H.x * 0.45, -0.4, face * H.z * 0.82))
                        * Mat4::from_scale(Vec3::new(0.16, 0.08, 0.05)), color: Vec3::new(0.85, 0.9, 0.95), emit: 0.4, mode: 0.0 });
                }
            }
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * (H.y * 1.92 + 0.12))
                * Mat4::from_scale(Vec3::new(H.x * 1.9, 0.07, H.z * 1.2)), color: team, emit: 0.35 + 0.3 * hurt, mode: 0.0 });
        }
        Behavior::Field => {
            for s in [-1.0f32, 1.0] {
                lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::new(s * panel.x, panel.y, 0.0))
                    * Mat4::from_scale(Vec3::new(0.14, panel.y * 2.0 + 0.2, 0.3)), color: dark, emit: 0.0, mode: 0.0 });
            }
            let pulse = 0.8 + 0.2 * (time * 3.0 + e.pos.x).sin();
            let col = [team.x, team.y, team.z, 0.28 * pulse * (0.4 + 0.6 * hurt)];
            emit.push(EmitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * panel.y)
                * Mat4::from_scale(Vec3::new(panel.x * 2.0, panel.y * 2.0, 0.06)), color: col });
        }
        Behavior::Pylon | Behavior::Sensor | Behavior::Camera | Behavior::Beacon => {
            use crate::sim::deploy::POST_HEIGHT;
            let look = crate::sim::placed::look(e.kind);
            let height = if look == Behavior::Beacon { 0.9 } else { POST_HEIGHT };
            // A post on a small foot.
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * 0.06)
                * Mat4::from_scale(Vec3::new(0.6, 0.12, 0.6)), color: dark, emit: 0.0, mode: 0.0 });
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * height * 0.5)
                * Mat4::from_scale(Vec3::new(0.16, height, 0.16)), color: steel * (0.6 + 0.4 * hurt), emit: 0.0, mode: 0.0 });
            let top = e.pos + Vec3::Y * height;
            let pulse = 0.6 + 0.4 * (time * 3.0 + e.pos.x).sin().abs();
            match look {
                Behavior::Sensor => {
                    // A turning dish.
                    let dish = Mat4::from_translation(top) * Mat4::from_rotation_y(time * 1.5 + e.pos.z) * Mat4::from_rotation_x(-0.5);
                    lit.push(LitDraw { mesh: MeshId::Disc, model: dish * Mat4::from_scale(Vec3::new(0.45, 0.05, 0.45)), color: steel, emit: 0.0, mode: 0.0 });
                    lit.push(LitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(top) * Mat4::from_scale(Vec3::splat(0.09)), color: team, emit: pulse, mode: 0.0 });
                }
                Behavior::Camera => {
                    // A lens box looking along its facing.
                    let head = base * Mat4::from_translation(Vec3::Y * height);
                    lit.push(LitDraw { mesh: MeshId::Bevel, model: head * Mat4::from_scale(Vec3::new(0.22, 0.2, 0.36)), color: dark, emit: 0.0, mode: 0.0 });
                    lit.push(LitDraw { mesh: MeshId::Disc, model: head * Mat4::from_translation(Vec3::new(0.0, 0.0, 0.19))
                        * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2) * Mat4::from_scale(Vec3::new(0.08, 0.02, 0.08)), color: team, emit: 1.0, mode: 0.0 });
                }
                _ => {
                    // A glowing orb; a pylon's aura shows as a faint ring.
                    let blink = if look == Behavior::Beacon { if (time * 2.0).fract() < 0.5 { 1.4 } else { 0.2 } } else { pulse };
                    lit.push(LitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(top + Vec3::Y * 0.12) * Mat4::from_scale(Vec3::splat(0.2)),
                        color: team, emit: blink, mode: 0.0 });
                    if let Some(a) = e.kind.def().aura.as_ref() {
                        emit.push(EmitDraw { mesh: MeshId::Disc, model: Mat4::from_translation(e.pos + Vec3::Y * 0.08)
                            * Mat4::from_scale(Vec3::new(a.range, 0.01, a.range)), color: [team.x, team.y, team.z, 0.05 * pulse] });
                    }
                }
            }
        }
        Behavior::Pad => {
            // A low plate with glowing chevrons pointing the way it throws.
            let h = crate::sim::deploy::half_extents(e.kind).unwrap_or(crate::sim::deploy::PAD_HALF);
            lit.push(LitDraw { mesh: MeshId::Bevel, model: base * Mat4::from_translation(Vec3::Y * h.y) * Mat4::from_scale(h * 2.0),
                color: steel * (0.6 + 0.4 * hurt), emit: 0.0, mode: 0.0 });
            if e.kind.def().pad.is_some() {
                let glow = 0.6 + 0.6 * (time * 4.0 + e.pos.x).sin().abs();
                for k in 0..3 {
                    let z = (k as f32 - 1.0) * h.z * 0.5;
                    lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::new(0.0, h.y * 2.0 + 0.01, -z))
                        * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_4) * Mat4::from_scale(Vec3::new(h.x * 0.5, 0.02, 0.12)),
                        color: team, emit: glow, mode: 0.0 });
                }
            } else {
                lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * (h.y * 2.0 + 0.01))
                    * Mat4::from_scale(Vec3::new(h.x * 2.02, 0.02, 0.1)), color: team, emit: 0.3, mode: 0.0 });
            }
        }
        Behavior::Gate => {
            // Two posts and a lintel, with a shimmering sheet between.
            let h = crate::sim::deploy::half_extents(e.kind).unwrap_or(crate::sim::deploy::GATE_HALF);
            for s in [-1.0f32, 1.0] {
                lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::new(s * h.x, h.y, 0.0))
                    * Mat4::from_scale(Vec3::new(0.18, h.y * 2.0, 0.3)), color: dark, emit: 0.0, mode: 0.0 });
            }
            lit.push(LitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * (h.y * 2.0))
                * Mat4::from_scale(Vec3::new(h.x * 2.2, 0.18, 0.32)), color: team, emit: 0.4, mode: 0.0 });
            let shimmer = 0.18 + 0.08 * (time * 5.0 + e.pos.z).sin();
            emit.push(EmitDraw { mesh: MeshId::Cube, model: base * Mat4::from_translation(Vec3::Y * h.y)
                * Mat4::from_scale(Vec3::new(h.x * 2.0, h.y * 2.0, 0.04)), color: [team.x, team.y, team.z, shimmer * (0.4 + 0.6 * hurt)] });
        }
    }
}

/// Where the first-person repair tool sits in view space (where the weapons
/// do, `sim::vm_anchor`), and its emitter tip.
const REPAIR_TOOL_VM: Vec3 = Vec3::new(0.27, -0.19, -0.62);
const REPAIR_TOOL_SCALE: f32 = 0.62;
const REPAIR_TIP_VM: Vec3 = Vec3::new(REPAIR_TOOL_VM.x, REPAIR_TOOL_VM.y, REPAIR_TOOL_VM.z - 0.41 * REPAIR_TOOL_SCALE);

/// A control point's light colour: its owner's (Ember red, Glacier blue,
/// neutral white), shifting toward the capturing team's as progress fills. Red
/// and blue pass through purple on the way, not through a muddy grey.
pub fn beacon_color(p: &peakrunner_core::control::Point) -> Vec3 {
    let rgb = |t: Option<u8>| match t {
        Some(0) => Vec3::new(1.0, 0.16, 0.08),
        Some(1) => Vec3::new(0.1, 0.45, 1.0),
        _ => Vec3::new(0.86, 0.88, 0.92),
    };
    let from = rgb(p.owner);
    let Some(to) = p.capturing.filter(|_| p.progress > 0.0) else { return from };
    let t = p.progress.clamp(0.0, 1.0);
    let to_rgb = rgb(Some(to));
    if p.owner.is_some() {
        let purple = Vec3::new(0.62, 0.12, 0.95);
        if t < 0.5 { from.lerp(purple, t * 2.0) } else { purple.lerp(to_rgb, t * 2.0 - 1.0) }
    } else {
        from.lerp(to_rgb, t)
    }
}

/// A beacon light on top of whatever stands at each active control point (a
/// tower's crown, or floating over open ground), or at its own height over
/// the point (`Point::beacon`, a post's crown), coloured by `beacon_color`.
/// Contested points flicker.
fn push_beacons(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, world: &World) {
    for (k, p) in world.points.iter().enumerate().filter(|(_, p)| p.active) {
        let at = match p.beacon {
            Some(h) => p.pos + Vec3::Y * h,
            None => Vec3::new(p.pos.x, peakrunner_core::terrain::support_on(world.map, p.pos + Vec3::Y * 60.0).0 + 1.8, p.pos.z),
        };
        let col = beacon_color(p);
        let flicker = if p.contested { 0.55 + 0.45 * (world.time * 14.0 + k as f32).sin().abs() } else { 1.0 };
        let spin = world.time * 0.6 + k as f32;
        lit.push(LitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_rotation_y(spin)
            * Mat4::from_scale(Vec3::new(0.45, 0.85, 0.45)), color: col, emit: 1.6 * flicker, mode: 0.0 });
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(1.7)),
            color: [col.x, col.y, col.z, -0.35 * flicker] });
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at + Vec3::Y * 6.0)
            * Mat4::from_scale(Vec3::new(0.12, 6.0, 0.12)), color: [col.x, col.y, col.z, 0.18 * flicker] });
    }
}

/// Spire tips on active control points glow in the holder's color (pale
/// and dim while the point is neutral, flickering while it's contested), and
/// strike lightning at every enemy they drain (`World::point_drain`).
fn push_spires(emit: &mut Vec<EmitDraw>, world: &World) {
    if world.points.iter().all(|p| !p.active || p.spires.is_empty()) { return; }
    for (k, p) in world.points.iter().enumerate().filter(|(_, p)| p.active && !p.spires.is_empty()) {
        let col = beacon_color(p);
        let (core, halo, bright) = if p.owner.is_some() { (0.55, 2.2, 1.6) } else { (0.26, 0.8, 0.7) };
        for (j, tip) in p.spires.iter().enumerate() {
            let flicker = if p.contested { 0.5 + 0.5 * (world.time * 14.0 + j as f32 * 1.3).sin().abs() }
                else { 0.85 + 0.15 * (world.time * 2.6 + k as f32 + j as f32 * 0.8).sin() };
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(*tip) * Mat4::from_scale(Vec3::splat(core)),
                color: [0.4 + col.x * 0.6, 0.4 + col.y * 0.6, 0.4 + col.z * 0.6, bright * flicker] });
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(*tip) * Mat4::from_scale(Vec3::splat(halo)),
                color: [col.x, col.y, col.z, -0.5 * flicker] });
        }
    }
    for i in 0..world.players.len() {
        let Some(d) = world.point_drain(i) else { continue };
        let Some(tip) = d.spire else { continue };
        let col = beacon_color(&world.points[d.point]);
        push_bolt(emit, tip, world.players[i].pos + Vec3::Y * 0.4, col, world.time, i);
    }
}

/// A lightning bolt from `from` to `to`: a jagged line that re-forks about
/// fifteen times a second, a white-hot core in a glow of `col`, with a
/// short side branch and a flash where it lands.
fn push_bolt(emit: &mut Vec<EmitDraw>, from: Vec3, to: Vec3, col: Vec3, time: f32, seed: usize) {
    let span = to - from;
    let len = span.length();
    if len < 0.5 { return; }
    let frame = (time * 15.0).floor();
    let hash = |n: f32| { let x = (n * 12.9898 + seed as f32 * 78.233 + frame * 37.719).sin() * 43758.547; x - x.floor() - 0.5 };
    let side = span.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    let up = side.cross(span / len);
    let steps = ((len / 3.0).ceil() as usize).clamp(4, 16);
    let jag = (len * 0.06).clamp(0.4, 2.2);
    let points: Vec<Vec3> = (0..=steps).map(|k| {
        let t = k as f32 / steps as f32;
        let edge = (t * (1.0 - t) * 4.0).min(1.0);
        from + span * t + (side * hash(k as f32) + up * hash(k as f32 + 50.0)) * jag * edge
    }).collect();
    let core = [0.75 + col.x * 0.25, 0.75 + col.y * 0.25, 0.75 + col.z * 0.25, 1.6];
    let glow = [col.x, col.y, col.z, -0.55];
    let mut segment = |a: Vec3, b: Vec3, width: f32| {
        let d = b - a;
        let l = d.length();
        if l < 0.05 { return; }
        let along = Mat4::from_translation(a + d * 0.5) * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::Y, d / l));
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: along * Mat4::from_scale(Vec3::new(width, l * 0.5, width)), color: core });
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: along * Mat4::from_scale(Vec3::new(width * 5.0, l * 0.5, width * 5.0)), color: glow });
    };
    for w in points.windows(2) { segment(w[0], w[1], 0.09); }
    // A branch off a third of the way down, gone again next fork.
    let k = steps / 3;
    let fork = points[k] + (side * hash(90.0) + up * hash(91.0) + span / len * 0.4) * jag * 2.2;
    segment(points[k], fork, 0.05);
    emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(to) * Mat4::from_scale(Vec3::splat(0.9)),
        color: [col.x, col.y, col.z, -0.6] });
}

/// A dropped ammo pack: a small olive pack with a glowing strap; it blinks
/// in its last seconds before it vanishes.
fn push_loot(lit: &mut Vec<LitDraw>, l: &crate::sim::loadout::Loot, riding: Option<Vec3>, time: f32) {
    let left = crate::sim::loadout::LOOT_SECONDS - l.age;
    if left < 4.0 && (time * 6.0).fract() < 0.35 { return; }
    // Riding a body down it tumbles with it; at rest it hovers and turns.
    let bob = match riding {
        Some(at) => Mat4::from_translation(at + Vec3::Y * 0.3) * Mat4::from_rotation_y(time * 2.3) * Mat4::from_rotation_x(time * 3.1),
        None => Mat4::from_translation(l.pos + Vec3::Y * (0.28 + 0.05 * (time * 2.0).sin())) * Mat4::from_rotation_y(time * 0.8),
    };
    // An item, a dropped weapon or a dropped pack (`items`) instead of rounds.
    if l.item > 0 || l.weapon > 0 || l.pack > 0 {
        push_loot_extra(lit, l, bob, time);
        return;
    }
    lit.push(LitDraw { mesh: MeshId::Bevel, model: bob * Mat4::from_scale(Vec3::new(0.5, 0.38, 0.32)),
        color: Vec3::new(0.3, 0.33, 0.2), emit: 0.0, mode: 0.0 });
    lit.push(LitDraw { mesh: MeshId::Cube, model: bob * Mat4::from_scale(Vec3::new(0.52, 0.07, 0.34)),
        color: Vec3::new(1.0, 0.78, 0.25), emit: 0.6, mode: 0.0 });
}

/// An item by its look, a dropped weapon (a long case) or a dropped pack.
fn push_loot_extra(lit: &mut Vec<LitDraw>, l: &crate::sim::loadout::Loot, bob: Mat4, time: f32) {
    use peakrunner_core::registry::ItemLook;
    let glow = 0.5 + 0.3 * (time * 3.0).sin().abs();
    if l.weapon > 0 {
        lit.push(LitDraw { mesh: MeshId::Bevel, model: bob * Mat4::from_scale(Vec3::new(0.18, 0.16, 0.9)), color: Vec3::new(0.25, 0.27, 0.3), emit: 0.0, mode: 0.0 });
        lit.push(LitDraw { mesh: MeshId::Cube, model: bob * Mat4::from_translation(Vec3::Y * 0.09) * Mat4::from_scale(Vec3::new(0.19, 0.03, 0.6)),
            color: Vec3::new(0.95, 0.85, 0.5), emit: glow, mode: 0.0 });
        return;
    }
    if l.pack > 0 {
        lit.push(LitDraw { mesh: MeshId::Bevel, model: bob * Mat4::from_scale(Vec3::new(0.55, 0.6, 0.3)), color: Vec3::new(0.3, 0.32, 0.36), emit: 0.0, mode: 0.0 });
        lit.push(LitDraw { mesh: MeshId::Cube, model: bob * Mat4::from_translation(Vec3::new(0.0, 0.0, 0.16)) * Mat4::from_scale(Vec3::new(0.3, 0.3, 0.02)),
            color: Vec3::new(0.4, 0.8, 1.0), emit: glow, mode: 0.0 });
        return;
    }
    let item = peakrunner_core::registry::registry().items.get(l.item as usize - 1);
    if let Some(parts) = item.map(|t| &t.parts).filter(|p| !p.is_empty()) {
        let mut glow = Vec::new();
        push_parts(lit, &mut glow, bob, parts, team_accent(Team::Ember));
        return;
    }
    let look = item.map_or(ItemLook::Kit, |t| t.look);
    match look {
        ItemLook::Kit => {
            // A white case with a green band.
            lit.push(LitDraw { mesh: MeshId::Bevel, model: bob * Mat4::from_scale(Vec3::new(0.45, 0.3, 0.3)), color: Vec3::new(0.85, 0.87, 0.88), emit: 0.0, mode: 0.0 });
            lit.push(LitDraw { mesh: MeshId::Cube, model: bob * Mat4::from_scale(Vec3::new(0.47, 0.08, 0.32)), color: Vec3::new(0.3, 0.95, 0.45), emit: glow, mode: 0.0 });
        }
        ItemLook::Box => {
            lit.push(LitDraw { mesh: MeshId::Bevel, model: bob * Mat4::from_scale(Vec3::new(0.5, 0.35, 0.35)), color: Vec3::new(0.32, 0.3, 0.2), emit: 0.0, mode: 0.0 });
            lit.push(LitDraw { mesh: MeshId::Cube, model: bob * Mat4::from_scale(Vec3::new(0.52, 0.06, 0.37)), color: Vec3::new(1.0, 0.6, 0.2), emit: glow, mode: 0.0 });
        }
        ItemLook::Cell => {
            lit.push(LitDraw { mesh: MeshId::Capsule, model: bob * Mat4::from_scale(Vec3::new(0.14, 0.32, 0.14)), color: Vec3::new(0.3, 0.75, 1.0), emit: 0.4 + glow, mode: 0.0 });
        }
    }
}

/// Repair tool beams: a pulsing green line from each repairer's tool to
/// what they're fixing.
/// Held beams (`firing`, a `stream` weapon): a crackling line from the
/// muzzle to where it lands, violet for one that drains or hurts, green for
/// one that only mends; a marking one (the target marker's) a steady thin
/// laser in its round's `color`.
fn push_stream_beams(emit: &mut Vec<EmitDraw>, world: &World) {
    let registry = peakrunner_core::registry::registry();
    for (i, p) in world.players.iter().enumerate() {
        let Some(to) = p.stream.filter(|_| p.alive) else { continue };
        let round = registry.weapons.get(p.weapon as usize).and_then(|w| registry.projectiles.by_id(&w.projectile));
        let mends = round.and_then(|r| r.stream).is_some_and(|s| s.damage == 0.0 && s.drain == 0.0);
        let marking = round.filter(|r| r.marks > 0.0);
        let (eye, look) = if i == world.player_id && !third_person_view(world) { let (e, l, _) = world.camera(); (e, l) }
            else { (drawn_at(world, i) + Vec3::Y * peakrunner_core::terrain::EYE, Vec3::new(-p.yaw.sin() * p.pitch.cos(), p.pitch.sin(), -p.yaw.cos() * p.pitch.cos())) };
        let from = crate::sim::weapon_muzzle(eye, look, p.weapon, crate::sim::camera_fov(0.0));
        let dir = to - from;
        let len = dir.length();
        if len < 0.2 { continue; }
        let along = Mat4::from_translation(from + dir * 0.5) * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::Y, dir / len));
        if let Some(r) = marking {
            let glow = r.color.unwrap_or([1.0, 0.62, 0.18]);
            let core = [0.5 + glow[0] * 0.5, 0.5 + glow[1] * 0.5, 0.5 + glow[2] * 0.5];
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: along * Mat4::from_scale(Vec3::new(0.026, len * 0.5, 0.026)),
                color: [core[0], core[1], core[2], 0.95] });
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: along * Mat4::from_scale(Vec3::new(0.1, len * 0.5, 0.1)),
                color: [glow[0], glow[1], glow[2], -0.45] });
            let pulse = 0.85 + 0.15 * (world.time * 6.0).sin();
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(to) * Mat4::from_scale(Vec3::splat(0.12)),
                color: [core[0], core[1], core[2], 0.95] });
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(to) * Mat4::from_scale(Vec3::splat(0.4 * pulse)),
                color: [glow[0], glow[1], glow[2], 0.35] });
            continue;
        }
        let flicker = 0.55 + 0.45 * (world.time * 41.0 + i as f32 * 1.7).sin().abs();
        let (core, glow) = if mends { ([0.4, 1.0, 0.5], [0.2, 0.9, 0.35]) } else { ([0.78, 0.55, 1.0], [0.5, 0.25, 1.0]) };
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: along * Mat4::from_scale(Vec3::new(0.04, len * 0.5, 0.04)),
            color: [core[0], core[1], core[2], 0.95 * flicker] });
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: along * Mat4::from_scale(Vec3::new(0.16, len * 0.5, 0.16)),
            color: [glow[0], glow[1], glow[2], -0.3] });
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(to) * Mat4::from_scale(Vec3::splat(0.35 * flicker)),
            color: [core[0], core[1], core[2], 0.6] });
    }
}

/// Your own body is on screen, seen from behind (third person, not looking
/// through a sight): what you hold and fire comes from it, not from the
/// first-person view.
pub(crate) fn third_person_view(world: &World) -> bool { world.third_person && !world.zoom_active() }

/// Where player `i`'s body is drawn: between steps (`World::drawn_offset`),
/// your own as the camera follows it.
pub(crate) fn drawn_at(world: &World, i: usize) -> Vec3 {
    world.players[i].pos + world.drawn_offset(i)
}

fn push_repair_beams(emit: &mut Vec<EmitDraw>, world: &World) {
    for (i, p) in world.players.iter().enumerate() {
        let Some(to) = p.repair_beam.filter(|_| p.alive) else { continue };
        // Repairing yourself: a glow on the body (`push_body_extras`), no
        // stream. Judged by distance it used to draw one back to where a
        // moving player's snapshot had them.
        if p.repair_self { continue; }
        let right = Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin());
        let from = if i == world.player_id && !third_person_view(world) {
            // From the first-person tool's emitter, placed the way the
            // viewmodel's narrower field of view draws it.
            let (eye, look, fov) = world.camera();
            let widen = (fov * 0.5).to_radians().tan() / (crate::sim::VM_FOV * 0.5).to_radians().tan();
            let side = look.cross(Vec3::Y).normalize_or(right);
            let up = side.cross(look).normalize_or(Vec3::Y);
            eye + side * REPAIR_TIP_VM.x * widen + up * REPAIR_TIP_VM.y * widen + look * -REPAIR_TIP_VM.z
        } else { crate::player_model::drawn_frame(p, drawn_at(world, i)).transform_point3(Vec3::new(0.2, 1.3, -0.75)) };
        let dir = to - from;
        let len = dir.length();
        if len < 0.2 { continue; }
        let pulse = 0.6 + 0.4 * (world.time * 18.0 + i as f32).sin();
        let along = Mat4::from_translation(from + dir * 0.5) * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::Y, dir / len));
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: along * Mat4::from_scale(Vec3::new(0.035, len * 0.5, 0.035)),
            color: [0.4, 1.0, 0.5, 0.9 * pulse] });
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: along * Mat4::from_scale(Vec3::new(0.12, len * 0.5, 0.12)),
            color: [0.2, 0.9, 0.35, -0.3] });
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(to) * Mat4::from_scale(Vec3::splat(0.3 * pulse)),
            color: [0.5, 1.0, 0.6, 0.6] });
    }
}

/// Turret plasma: a flickering hot core inside two halo shells, and a trail.
/// The plasma gun's bolt: a hot orange ball in a soft halo with a short wake.
fn push_plasma_bolt(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc, time: f32) {
    let flicker = 0.9 + 0.1 * (time * 41.0 + d.pos.z * 2.7).sin();
    let [r, g, b] = PLASMA_GLOW.to_array();
    lit.push(LitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(d.pos) * Mat4::from_scale(Vec3::splat(0.2 * flicker)),
        color: Vec3::new(1.0, 0.86, 0.6), emit: 1.6, mode: 0.0 });
    emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(d.pos) * Mat4::from_scale(Vec3::splat(0.42 * flicker)),
        color: [r, g, b, 0.55] });
    let flown = (peakrunner_core::defs::weapons().plasma.life - d.life).max(0.0);
    for k in 1..=5 {
        let t = k as f32 / 5.0;
        let center = d.pos - d.vel * flown.min(0.05) * t;
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(center) * Mat4::from_scale(Vec3::splat(0.3 * (1.0 - t * 0.7))),
            color: [r, g * 0.8, b, 0.28 * (1.0 - t * 0.8)] });
    }
}

/// A blaster bolt: a short bright violet streak.
fn push_blaster_bolt(emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc) {
    let f = d.vel.normalize_or(Vec3::NEG_Z);
    let mut r = f.cross(Vec3::Y).normalize_or_zero();
    if r.length_squared() < 0.01 { r = Vec3::X; }
    let u = r.cross(f).normalize_or_zero();
    let streak = |w: f32, len: f32, color: [f32; 4]| EmitDraw { mesh: MeshId::Sphere,
        model: Mat4::from_cols((r * w).extend(0.0), (u * w).extend(0.0), (f * len * 0.5).extend(0.0), (d.pos - f * len * 0.4).extend(1.0)), color };
    let [cr, cg, cb] = BLASTER_GLOW.to_array();
    emit.push(streak(0.04, 0.9, [1.0, 0.92, 1.0, 1.0]));
    emit.push(streak(0.11, 1.6, [cr, cg, cb, 0.5]));
}

fn push_plasma(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc, time: f32) {
    let flicker = 0.88 + 0.12 * (time * 37.0 + d.pos.x * 3.1).sin();
    lit.push(LitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(d.pos) * Mat4::from_scale(Vec3::splat(0.32 * flicker)),
        color: Vec3::new(0.85, 1.0, 0.7), emit: 1.3, mode: 0.0 });
    emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(d.pos) * Mat4::from_scale(Vec3::splat(0.55)),
        color: [0.35, 1.0, 0.15, 0.55] });
    emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(d.pos) * Mat4::from_scale(Vec3::splat(0.95 * flicker)),
        color: [0.2, 0.9, 0.1, -0.35] });
    for k in 1..=7 {
        let t = k as f32 / 7.0;
        let center = d.pos - d.vel * (3.0 - d.life).clamp(0.0, 0.06) * t;
        emit.push(EmitDraw { mesh: MeshId::Sphere,
            model: Mat4::from_translation(center) * Mat4::from_scale(Vec3::splat(0.42 * (1.0 - t * 0.75))),
            color: [0.35, 1.0, 0.1, 0.3 * (1.0 - t * 0.8)] });
    }
}

/// Chaingun round. Every third round of a burst is a long bright tracer with
/// a soft glow; the rest are faint streaks, which gives the stream a rhythm.
/// A laser shot: a bright core and a soft team-tinted glow along the beam
/// (`vel` holds start to end), fading over its short life.
fn push_laser_beam(emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc, team: Vec3) {
    let length = d.vel.length();
    if length < 0.1 { return; }
    let f = d.vel/length;
    let mut r = f.cross(Vec3::Y).normalize_or_zero();
    if r.length_squared() < 0.01 { r = Vec3::X; }
    let u = r.cross(f).normalize_or_zero();
    let fade = (d.life/peakrunner_core::defs::weapons().laser.fade).clamp(0.0, 1.0);
    let beam = |mesh, w: f32, color: [f32; 4]| EmitDraw { mesh,
        model: Mat4::from_cols((r*w).extend(0.0), (u*w).extend(0.0), (f*length*if mesh == MeshId::Sphere { 0.5 } else { 1.0 }).extend(0.0),
            (d.pos+d.vel*0.5).extend(1.0)), color };
    emit.push(beam(MeshId::Cube, 0.03, [0.9, 1.0, 1.0, 0.95*fade]));
    emit.push(beam(MeshId::Sphere, 0.14, [0.3+team.x*0.2, 0.85, 1.0, 0.35*fade]));
    emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(d.pos+d.vel)*Mat4::from_scale(Vec3::splat(0.5*fade+0.1)),
        color: [0.8, 1.0, 1.0, 0.6*fade] });
}

/// A railgun slug: a hot white-blue dart with a long ionised streak behind it.
fn push_rail_slug(emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc) {
    let f = d.vel.normalize_or(Vec3::NEG_Z);
    let mut r = f.cross(Vec3::Y).normalize_or_zero();
    if r.length_squared() < 0.01 { r = Vec3::X; }
    let u = r.cross(f).normalize_or_zero();
    // A bright slug you can follow in flight, with a short wake (it flies
    // at 300 m/s, not as a beam).
    let flown = (peakrunner_core::defs::weapons().railgun.life-d.life)*d.vel.length();
    let length = flown.min(9.0);
    let streak = |w: f32, len: f32, color: [f32; 4]| EmitDraw { mesh: MeshId::Sphere,
        model: Mat4::from_cols((r*w).extend(0.0), (u*w).extend(0.0), (f*len*0.5).extend(0.0), (d.pos-f*len*0.5).extend(1.0)), color };
    emit.push(streak(0.09, 1.5, [1.0, 1.0, 1.0, 1.0]));
    emit.push(streak(0.22, 0.9, [0.55, 0.85, 1.0, 0.5]));
    if length > 0.5 {
        emit.push(streak(0.05, length, [0.55, 0.8, 1.0, 0.55]));
        emit.push(streak(0.16, length*0.6, [0.3, 0.55, 1.0, 0.16]));
    }
}

fn push_tracer(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, d: &crate::sim::Disc, time: f32) {
    // A round at rest still needs a full basis; a zero axis gives the lit
    // body a singular normal matrix, which shades as NaN (black).
    let f = d.vel.normalize_or(Vec3::NEG_Z);
    let mut r = f.cross(Vec3::Y).normalize_or_zero();
    if r.length_squared() < 0.01 { r = Vec3::X; }
    let u = r.cross(f).normalize_or_zero();
    // time + life is constant over a round's flight, so it names the round.
    let bright = (((time + d.life) / 0.075).round() as i64).rem_euclid(3) == 0;
    let (cap, max_len) = if bright { (0.012, 4.5) } else { (0.006, 2.0) };
    let length = (d.vel.length() * (1.2 - d.life).clamp(0.0, cap)).min(max_len);
    if length > 0.01 {
        let streak = |w: f32, color: [f32; 4]| EmitDraw { mesh: MeshId::Sphere,
            model: Mat4::from_cols((r * w).extend(0.0), (u * w).extend(0.0),
                (f * length * 0.5).extend(0.0), (d.pos - f * length * 0.5).extend(1.0)),
            color };
        if bright {
            emit.push(streak(0.035, [1.0, 0.82, 0.4, 0.9]));
            emit.push(streak(0.09, [1.0, 0.55, 0.15, 0.22]));
        } else {
            emit.push(streak(0.022, [1.0, 0.68, 0.22, 0.4]));
        }
    }
    lit.push(LitDraw {
        mesh: MeshId::Cube,
        model: Mat4::from_cols((r * 0.018).extend(0.0), (u * 0.018).extend(0.0), (f * 0.30).extend(0.0), d.pos.extend(1.0)),
        color: Vec3::new(1.0, 0.78, 0.30),
        emit: if bright { 1.2 } else { 0.8 },
        mode: 0.0,
    });
}

/// Flame burst colours: outer and inner lobes (rgb + strength), core and sparks.
struct Burst { extent: f32, outer: [f32; 4], inner: [f32; 4], core: [f32; 3], spark: [f32; 3] }
const BURST_BLUE: &Burst = &Burst { extent: 2.2, outer: [0.04, 0.22, 1.0, 0.65], inner: [0.35, 0.85, 1.0, 0.9],
    core: [0.7, 0.95, 1.0], spark: [0.3, 0.75, 1.0] };
const BURST_FIRE: &Burst = &Burst { extent: 3.2, outer: [1.0, 0.07, 0.005, 0.7], inner: [1.0, 0.75, 0.04, 0.95],
    core: [1.0, 0.95, 0.4], spark: [1.0, 0.4, 0.02] };
const BURST_GREEN: &Burst = &Burst { extent: 3.8, outer: [0.08, 0.75, 0.05, 0.7], inner: [0.55, 1.0, 0.3, 0.95],
    core: [0.85, 1.0, 0.6], spark: [0.4, 1.0, 0.2] };

fn flame_burst(emit:&mut Vec<EmitDraw>,pos:Vec3,age:f32,c:&Burst) {
    let t=(age/0.55).clamp(0.,1.);let fade=(1.-t).powi(2);
    let extent=c.extent;
    // Separate soft-edged tongues of fire, not one large explosion sphere.
    // The existing negative-alpha shader mode softens each lobe's silhouette.
    for i in 0..7 {
        let angle=i as f32*2.399+pos.x*0.13+pos.z*0.07;
        let axis=Vec3::new(angle.cos(),0.25+(i%3) as f32*0.35,angle.sin()).normalize();
        let center=pos+axis*extent*(0.15+t*1.2)+Vec3::Y*t*t*2.;
        let size=(0.55+(i%3) as f32*0.14)*(0.6+(t*5.).min(1.))*(1.-t*0.6);
        let model=Mat4::from_translation(center)*Mat4::from_rotation_z(angle*0.2)
            *Mat4::from_scale(Vec3::new(size,size*(1.5+t),size));
        emit.push(EmitDraw {mesh:MeshId::Sphere,model,
            color:[c.outer[0],c.outer[1],c.outer[2],-fade*c.outer[3]]});
        emit.push(EmitDraw {mesh:MeshId::Sphere,model:model*Mat4::from_scale(Vec3::splat(0.58)),
            color:[c.inner[0],c.inner[1],c.inner[2],-fade*c.inner[3]]});
    }
    emit.push(EmitDraw {mesh:MeshId::Sphere,
        model:Mat4::from_translation(pos)*Mat4::from_scale(Vec3::splat(0.65+t)),
        color:[c.core[0],c.core[1],c.core[2],-fade]});
    for i in 0..5 {
        let angle=i as f32*2.399;
        let direction=Vec3::new(angle.cos(),0.4+(i%2) as f32*0.5,angle.sin()).normalize();
        let center=pos+direction*(0.3+t*extent*3.)-Vec3::Y*t*t;
        emit.push(EmitDraw {mesh:MeshId::Sphere,
            model:Mat4::from_translation(center)*Mat4::from_scale(Vec3::new(0.045,0.16,0.045)),
            color:[c.spark[0],c.spark[1],c.spark[2],fade]});
    }
}

fn spinning_disc(pos: Vec3, vel: Vec3, spin: f32, radius: f32, thick: f32) -> Mat4 {
    let mut axis = vel.normalize_or_zero();
    if axis.length_squared() < 0.01 {
        axis = Vec3::Z;
    }
    let mut side = axis.cross(Vec3::Y);
    if side.length_squared() < 1e-4 {
        side = Vec3::X;
    }
    side = side.normalize();
    let up = side.cross(axis);
    let (s, c) = spin.sin_cos();
    let r = side * c - axis * s;
    let u = -side * s - axis * c;
    Mat4::from_cols(
        (r * radius).extend(0.0),
        (up * thick).extend(0.0),
        (u * radius).extend(0.0),
        pos.extend(1.0),
    )
}

fn disc_launcher(base: Mat4, time: f32, cooldown: f32, shot_age: f32, ready_flash: f32, loaded: bool, team: Vec3) -> Vec<LitDraw> {
    // Long silver split rails, recessed black feed bed,
    // cyan insets and yellow hazard marks. All parts are actual lit geometry.
    let silver = Vec3::new(0.66, 0.70, 0.73);
    let edge = Vec3::new(0.86, 0.89, 0.90);
    let steel = Vec3::new(0.29, 0.33, 0.37);
    let dark = Vec3::new(0.045, 0.065, 0.08);
    let cyan = Vec3::new(0.05, 0.74, 0.95);
    let mut draws = Vec::with_capacity(48);
    let mut part = |mesh, pos, scale, color, emit| {
        draws.push(LitDraw {
            mesh, model: base * Mat4::from_translation(pos) * Mat4::from_scale(scale),
            color, emit, mode: 0.0,
        });
    };
    part(MeshId::Bevel, Vec3::new(0.0, -0.055, 0.12), Vec3::new(0.30, 0.19, 0.42), silver, 0.0);
    part(MeshId::Bevel, Vec3::new(0.0, -0.22, 0.23), Vec3::new(0.13, 0.25, 0.21), dark, 0.0);
    part(MeshId::Bevel, Vec3::new(0.0, -0.04, -0.31), Vec3::new(0.40, 0.07, 0.70), steel, 0.0);
    part(MeshId::Cube, Vec3::new(0.0, 0.003, -0.27), Vec3::new(0.32, 0.012, 0.62), dark, 0.0);
    for side in [-1.0, 1.0] {
        let x = side * 0.205;
        part(MeshId::Bevel, Vec3::new(x, 0.018, -0.27), Vec3::new(0.09, 0.15, 0.98), silver, 0.0);
        part(MeshId::Bevel, Vec3::new(x, 0.088, -0.27), Vec3::new(0.063, 0.018, 0.91), edge, 0.0);
        part(MeshId::Bevel, Vec3::new(x, 0.018, -0.765), Vec3::new(0.085, 0.115, 0.055), steel, 0.0);
        // Interior rail energy strip; the open channel remains dark.
        part(MeshId::Cube, Vec3::new(side * 0.157, 0.035, -0.36), Vec3::new(0.008, 0.025, 0.50), cyan, 0.6);
        for k in 0..5 {
            let z = 0.04 - k as f32 * 0.14;
            part(MeshId::Cube, Vec3::new(x, 0.099, z), Vec3::new(0.047, 0.005, 0.05), dark, 0.0);
            part(MeshId::Bevel, Vec3::new(side * 0.251, 0.008, z), Vec3::new(0.01, 0.065, 0.065), steel, 0.0);
        }
        for k in 0..3 {
            part(MeshId::Cube, Vec3::new(x, 0.102, -0.49 - k as f32 * 0.065),
                Vec3::new(0.049, 0.01, 0.022), cyan, 0.7);
        }
    }
    // A mechanical feed sled opens and returns over the existing reload cycle.
    let cycle = (1.0 - cooldown / peakrunner_core::defs::weapons().disc.reload).clamp(0.0, 1.0);
    let open = if cooldown > 0.0 { (cycle * std::f32::consts::PI).sin() } else { 0.0 };
    part(MeshId::Bevel, Vec3::new(0.0, 0.065, 0.055 + open * 0.12),
        Vec3::new(0.17, 0.045, 0.19), steel, 0.0);
    part(MeshId::Cube, Vec3::new(0.0, 0.09, 0.08 + open * 0.12),
        Vec3::new(0.1, 0.008, 0.018), cyan, 0.35 + cycle * 0.25);
    for k in 0..4 {
        part(MeshId::Bevel, Vec3::new(-0.253, 0.031, 0.03 - k as f32 * 0.09),
            Vec3::new(0.013, 0.033, 0.041), Vec3::new(0.92, 0.66, 0.08), 0.0);
    }
    let charge = (1.0 - cooldown / 0.75).clamp(0.0, 1.0);
    if charge > 0.0 && loaded {
        // Expose the round in the widened tray: it should read as a
        // loaded disc launcher, not two bars concealing a tiny light.
        let disc_base = base * Mat4::from_translation(Vec3::new(0.0, 0.075, 0.12 - charge * 0.65))
            * Mat4::from_rotation_y(time * 18.0);
        draws.push(LitDraw {
            mesh: MeshId::Disc,
            model: disc_base * Mat4::from_scale(Vec3::new(0.155, 0.022, 0.155)),
            color: cyan, emit: 0.55, mode: 0.0,
        });
        draws.push(LitDraw {
            mesh: MeshId::Disc,
            model: disc_base * Mat4::from_translation(Vec3::Y * 0.012)
                * Mat4::from_scale(Vec3::new(0.132, 0.004, 0.132)),
            color: Vec3::new(0.025, 0.23, 0.95), emit: 0.5, mode: 0.0,
        });
    }
    // Charge bar along the top of the left rail, where the player sees it:
    // it fills rear to front over the reload and flashes once when ready.
    let fill = if !loaded { 0.0 } else if cooldown > 0.0 { 1.0 - cooldown / peakrunner_core::defs::weapons().disc.reload } else { 1.0 };
    let pulse = (1.0 - ready_flash / 0.25).max(0.0);
    for k in 0..8 {
        let on = (k as f32 + 0.5) / 8.0 <= fill;
        draws.push(LitDraw { mesh: MeshId::Cube,
            model: base * Mat4::from_translation(Vec3::new(-0.205, 0.104, 0.11 - k as f32 * 0.055))
                * Mat4::from_scale(Vec3::new(0.03, 0.006, 0.04)),
            color: if on { cyan } else { dark }, emit: if on { 0.7 + pulse * 1.3 } else { 0.0 }, mode: 0.0 });
    }
    draws.push(LitDraw { mesh: MeshId::Cube,
        model: base * Mat4::from_translation(Vec3::new(0.152, -0.05, 0.12)) * Mat4::from_scale(Vec3::new(0.004, 0.035, 0.34)),
        color: team, emit: 0.3, mode: 0.0 });
    // Muzzle crown: two short prongs at the rail ends that flare on fire.
    let flare = (-shot_age * 14.0).exp();
    for side in [-1.0, 1.0] {
        draws.push(LitDraw { mesh: MeshId::Bevel,
            model: base * Mat4::from_translation(Vec3::new(side * 0.205, 0.03, -0.835)) * Mat4::from_scale(Vec3::new(0.035, 0.05, 0.09)),
            color: silver, emit: 0.0, mode: 0.0 });
        draws.push(LitDraw { mesh: MeshId::Cube,
            model: base * Mat4::from_translation(Vec3::new(side * 0.205, 0.04, -0.885)) * Mat4::from_scale(Vec3::new(0.026, 0.02, 0.02)),
            color: cyan, emit: 0.25 + flare * 2.5, mode: 0.0 });
    }
    if shot_age < 0.08 {
        let k = 1.0 - shot_age / 0.08;
        draws.push(LitDraw {
            mesh: MeshId::Sphere,
            model: base * Mat4::from_translation(VM_MUZZLE_DISC)
                * Mat4::from_scale(Vec3::new(0.1, 0.035, 0.18) * (0.7 + k * 0.5)),
            color: Vec3::new(0.35, 0.85, 1.0), emit: 0.95, mode: 0.0,
        });
    }
    draws
}

fn push_base(lit: &mut Vec<LitDraw>, world: &World, ember: bool) {
    let spec = terrain::info(world.map);
    let home = if ember { spec.ember } else { spec.glacier };
    let y = terrain::height_on(world.map, home.x, home.z);
    let accent = if ember {
        Vec3::new(0.78, 0.22, 0.16)
    } else {
        Vec3::new(0.18, 0.62, 0.74)
    };
    let steel = Vec3::new(0.18, 0.20, 0.23);
    lit.push(LitDraw {
        mesh: MeshId::Cube,
        model: Mat4::from_translation(Vec3::new(home.x, y + 0.18, home.z))
            * Mat4::from_scale(Vec3::new(16.0, 0.4, 16.0)),
        color: steel,
        emit: 0.0,
        mode: 0.0,
    });
    let zoff = if ember { 4.0 } else { -4.0 };
    lit.push(LitDraw {
        mesh: MeshId::Cube,
        model: Mat4::from_translation(Vec3::new(home.x + 6.5, y + 5.5, home.z + zoff))
            * Mat4::from_scale(Vec3::new(2.4, 11.0, 2.4)),
        color: steel,
        emit: 0.0,
        mode: 0.0,
    });
    lit.push(LitDraw {
        mesh: MeshId::Cube,
        model: Mat4::from_translation(Vec3::new(home.x + 6.5, y + 11.2, home.z + zoff))
            * Mat4::from_scale(Vec3::new(3.0, 0.5, 3.0)),
        color: accent,
        emit: 0.25,
        mode: 0.0,
    });
    for k in 0..3 {
        let a = k as f32 * 2.1;
        lit.push(LitDraw {
            mesh: MeshId::Cube,
            model: Mat4::from_translation(Vec3::new(
                home.x + a.cos() * 7.5,
                y + 1.1,
                home.z + a.sin() * 7.5,
            )) * Mat4::from_scale(Vec3::new(1.6, 2.2, 1.6)),
            color: steel * 1.15,
            emit: 0.0,
            mode: 0.0,
        });
    }
}

fn team_accent(team: Team) -> Vec3 { crate::player_model::team_accent(team) }

/// A shot's trim: its team's, or where the mode has no teams (Rabbit,
/// free-for-all) a neutral one, so a shot names no side.
fn side_accent(world: &World, team: Team) -> Vec3 {
    if world.ffa() { crate::player_model::NEUTRAL_ACCENT } else { team_accent(team) }
}

/// A shot model's paint, the same way (`player_model::HUNTER_STEEL` where
/// there are no teams).
fn side_paint(world: &World, team: Team) -> Vec3 {
    if world.ffa() { crate::player_model::HUNTER_STEEL } else { crate::player_model::team_color(team) }
}

/// Per-weapon recoil read from the time since the shot: (push back, pitch
/// up, yaw jitter). Stateless, so snapshot replays and captures are exact.
pub(crate) fn recoil(weapon: u8, age: f32, time: f32) -> (f32, f32, f32) {
    match weapon {
        // Heavy shove: fast back, then a lagging pitch-up that settles.
        0 => { let k = (-age * 9.0).exp(); let rise = age * (-age * 7.0).exp() * 5.5; (0.10 * k, 0.14 * rise + 0.04 * k, 0.0) }
        // Fast, small jitter that keeps buzzing while the trigger cycles.
        1 => { let k = (-age * 40.0).exp(); (0.014 * k, 0.02 * k, 0.008 * (time * 97.0).sin() * k) }
        // Plasma gun: a soft thump. Blaster: a light, quick snap.
        crate::sim::arsenal::PLASMA => { let k = (-age * 11.0).exp(); (0.05 * k, 0.06 * k, 0.0) }
        crate::sim::arsenal::BLASTER => { let k = (-age * 26.0).exp(); (0.022 * k, 0.03 * k, 0.0) }
        // Big, slow shove with a long recovery.
        _ => { let k = (-age * 4.5).exp(); let rise = age * (-age * 5.0).exp() * 4.0; (0.09 * k, 0.26 * rise + 0.05 * k, 0.0) }
    }
}

/// The football: a small dark case with pale metal bands, a grille and a lamp. It
/// never spins or tumbles: it stays upright in flight and at rest. Its light on
/// the ground is `ball_lights`.
pub(crate) fn ball_draws(body: Mat4) -> Vec<LitDraw> {
    let case = Vec3::new(0.11, 0.09, 0.075);
    let band = Vec3::new(0.50, 0.46, 0.38);
    let mut draws = Vec::with_capacity(7);
    let mut put = |mesh, at: Vec3, size: Vec3, color: Vec3, emit: f32| draws.push(LitDraw { mesh,
        model: body * Mat4::from_translation(at) * Mat4::from_scale(size), color, emit, mode: 0.0 });
    put(MeshId::Bevel, Vec3::ZERO, Vec3::new(0.46, 0.30, 0.32), case, 0.0);
    for x in [-0.15, 0.15] { put(MeshId::Cube, Vec3::new(x, 0.0, 0.0), Vec3::new(0.05, 0.315, 0.335), band, 0.0); }
    put(MeshId::Cube, Vec3::new(0.0, 0.165, 0.0), Vec3::new(0.18, 0.03, 0.05), band, 0.0);
    for z in [-0.162, 0.162] {
        put(MeshId::Cube, Vec3::new(0.0, -0.02, z), Vec3::new(0.2, 0.13, 0.01), Vec3::splat(0.025), 0.0);
        put(MeshId::Cube, Vec3::new(0.0, 0.085, z), Vec3::new(0.07, 0.035, 0.012), Vec3::new(1.0, 0.95, 0.85), 1.2);
    }
    draws
}

/// A flag standing at `pos`: pole, finial, collar and a waving cloth of
/// `color` that keeps facing the viewer at `eye` (`phase` offsets its wave).
fn push_flag(lit: &mut Vec<LitDraw>, pos: Vec3, color: Vec3, eye: Vec3, time: f32, phase: f32) {
    let metal = Vec3::new(0.15, 0.16, 0.18);
    lit.push(LitDraw {
        mesh: MeshId::Cube,
        model: Mat4::from_translation(pos + Vec3::Y * 1.75)
            * Mat4::from_scale(Vec3::new(0.12, 3.5, 0.12)),
        color: metal, emit: 0.0, mode: 0.0,
    });
    // Finial and a collar where the cloth's hoist meets the pole.
    lit.push(LitDraw {
        mesh: MeshId::Sphere,
        model: Mat4::from_translation(pos + Vec3::Y * 3.58) * Mat4::from_scale(Vec3::splat(0.17)),
        color: Vec3::new(0.78, 0.62, 0.30), emit: 0.15, mode: 0.0,
    });
    lit.push(LitDraw {
        mesh: MeshId::Cube,
        model: Mat4::from_translation(pos + Vec3::Y * 3.32) * Mat4::from_scale(Vec3::new(0.2, 0.1, 0.2)),
        color: metal, emit: 0.0, mode: 0.0,
    });
    // The cloth keeps facing the viewer so it reads from range, and waves
    // in strips whose ripple grows toward the free edge.
    let mut to = eye - pos;
    to.y = 0.0;
    let to = to.normalize_or_zero();
    let right = to.cross(Vec3::Y).normalize_or_zero();
    const STRIPS: usize = 6;
    let (width, height) = (1.5, 1.1);
    let strip = width / STRIPS as f32;
    for i in 0..STRIPS {
        let u = (i as f32 + 0.5) / STRIPS as f32;
        let wave = time * 4.2 - u * 5.0 + phase;
        let sway = u * 0.22 * wave.sin();
        let droop = u * u * 0.12;
        let shade = 0.86 + 0.14 * (wave + 1.2).cos();
        let center = pos + Vec3::Y * (2.75 - droop) + right * (0.06 + u * width) + to * sway;
        lit.push(LitDraw {
            mesh: MeshId::Cube,
            model: Mat4::from_cols(
                (right * strip * 1.04).extend(0.0),
                (Vec3::Y * height).extend(0.0),
                (to * 0.06).extend(0.0),
                center.extend(1.0),
            ),
            color: color * shade,
            emit: 0.35,
            mode: 0.0,
        });
    }
}

/// A loose ball in the world. A carried one is drawn in its carrier's arm
/// (`player_model::Hold::Carrier`).
fn push_ball(lit: &mut Vec<LitDraw>, world: &World) {
    let b = &world.ball;
    if !b.loose() { return; }
    // The case sits on its base: its centre is under the ball's.
    let at = b.pos - Vec3::Y * (peakrunner_core::sim::football::BALL_RADIUS - 0.15);
    lit.extend(ball_draws(Mat4::from_translation(at)));
}

/// How far the ball's light reaches, and how often a carrier's pulses.
pub const BALL_LIGHT_RADIUS: f32 = 5.0;
const CARRIER_PULSE_SECONDS: f32 = 1.5;

/// The football's light: a loose ball shines steadily white and a carrier's
/// flag pulses every 1.5 s, lighting the ground round them so the play shows across the
/// field.
fn ball_lights(world: &World) -> [[f32; 4]; 4] {
    let mut lights = [[0.0; 4]; 4];
    let b = &world.ball;
    if !b.active || !b.in_play { return lights; }
    let (at, strength) = match b.carrier.map(|c| (c, world.players.get(c))) {
        Some((c, Some(p))) if p.alive => {
            let shift = world.drawn_offset(c);
            let pulse = 0.5 + 0.5 * (world.time * std::f32::consts::TAU / CARRIER_PULSE_SECONDS).cos();
            (p.pos + shift + Vec3::Y * 1.3, 0.6 + 0.4 * pulse)
        }
        Some(_) => return lights,
        None => (b.pos + Vec3::Y * 0.9, 1.0),
    };
    lights[0] = [at.x, at.y, at.z, BALL_LIGHT_RADIUS];
    lights[1] = [1.0, 0.97, 0.9, 1.6 * strength];
    lights
}

/// The first-person view's built-in parts and authored models.
fn viewmodel_draws(world: &World, anim: &ViewAnim) -> (Vec<LitDraw>, Vec<ModelDraw>) {
    let mut models = Vec::new();
    let lit = viewmodel_lit(world, anim, &mut models);
    (lit, models)
}

/// Where an authored weapon's right-hand grip sits in the first-person
/// weapon frame (the built-in models' pistol grip), and its scale there.
const VM_GRIP: Vec3 = Vec3::new(-0.03, -0.105, 0.13);
const VM_SCALE: f32 = 0.9;

fn viewmodel_lit(world: &World, anim: &ViewAnim, models: &mut Vec<ModelDraw>) -> Vec<LitDraw> {
    if world.state == MatchState::Flyby {
        return Vec::new();
    }
    let Some(p) = world.players.get(world.player_id) else {
        return Vec::new();
    };
    if !p.alive || p.stun > 0.0 || p.riding > 0 {
        return Vec::new();
    }
    // Your own arms and gear in the colors your body wears.
    let livery = crate::player_model::livery(world, world.player_id);
    // Watching from behind, the body holds what the hands do: nothing in
    // view (the repair tool and the deployer used to come up on screen).
    if third_person_view(world) { return Vec::new(); }
    // Football is played empty-handed: carrying, the ball is low on the
    // right in your armored hand; else nothing is in view.
    if world.ball.active {
        let mut draws = Vec::new();
        if world.ball.carried_by(world.player_id) {
            let frame = Mat4::from_translation(VM_BALL + anim.bob_offset(world.time)) * Mat4::from_rotation_y(0.3) * Mat4::from_rotation_x(0.15);
            push_vm_ball(&mut draws, models, frame, p, livery);
        }
        return draws;
    }
    // Lining up a pack: the deployer replaces the weapon.
    if world.placing {
        if let Some(kind) = p.pack.and_then(|k| k.deployable()) {
            let frame = Mat4::from_translation(Vec3::new(0.24, -0.22, -0.55) + anim.bob_offset(world.time)) * Mat4::from_rotation_y(0.12);
            let fits = world.deploy_ghost.as_ref().is_some_and(|g| g.1);
            let mut draws = deployer_draws(frame, kind, p.team, fits, world.time);
            push_vm_support(&mut draws, models, frame, vm_hand(1), p, livery);
            return draws;
        }
    }
    // The repair key held: the repair tool comes up instead of the weapon,
    // running or resting on an empty tank.
    if p.repair_out {
        let frame = Mat4::from_translation(REPAIR_TOOL_VM + anim.bob_offset(world.time)) * Mat4::from_rotation_y(0.01)
            * Mat4::from_scale(Vec3::splat(REPAIR_TOOL_SCALE));
        let mut draws = crate::player_model::repair_tool_parts(frame, world.time, p.repair_beam.is_some());
        // First person, the glow would bloom across the view: keep it low.
        for d in &mut draws { d.emit = d.emit.min(0.45); }
        push_vm_support(&mut draws, models, frame, vm_hand(1), p, livery);
        return draws;
    }
    // Zoomed through a sight, or watching from behind: no weapon in view; a
    // mode without weapons leaves the hands empty.
    if world.zoom_active() || world.third_person || !world.armed(world.player_id) { return Vec::new(); }
    // In a blade zone the blade is drawn in the world (`push_first_person_blade`).
    if world.blade_zone_of(world.player_id).is_some() { return Vec::new(); }
    let (shown, drop) = anim.shown();
    // The model it's drawn with, and that model's number in the tables.
    let view = crate::sim::arsenal::view(shown);
    let look = view.kind();
    // A weapon on its way down has no live cooldown of its own.
    let cooldown = if shown == p.weapon { p.cooldown } else { 0.0 };
    let reload = weapon_reload(shown);
    let shot_age = if cooldown > 0.0 { (reload - cooldown).max(0.0) } else { 9.0 };
    let loaded = match peakrunner_core::registry::registry().weapons.get(shown as usize) {
        Some(w) if w.energy > 0.0 => p.energy >= w.energy,
        _ => p.ammo.get(shown as usize).is_some_and(|&n| n > 0),
    };
    let (back, rise, jitter) = recoil(look, shot_age, world.time);
    let anchor = crate::sim::vm_anchor(view);
    let (yaw, pitch) = crate::sim::vm_turn(look);
    let offset = anim.bob_offset(world.time) + Vec3::new(0.0, -back * 0.4 - 0.2 * drop, back + 0.05 * drop);
    let base = Mat4::from_translation(anchor + offset)
        * Mat4::from_rotation_y(yaw + jitter)
        * Mat4::from_rotation_x(pitch + rise - drop * 0.45);
    let team = livery.accent;
    // An authored model, its right-hand grip in the hand and the support
    // hand on its left grip.
    if let Some((id, model)) = crate::player_model::weapon_model(shown) {
        let grip = Mat4::from_translation(VM_GRIP) * Mat4::from_scale(Vec3::splat(VM_SCALE))
            * model.point("grip_r").unwrap_or(Mat4::IDENTITY).inverse();
        let glow = if loaded { 1.0 } else { 0.25 };
        crate::model::push(models, id, model, base * grip, livery.color, glow, &|_| None);
        let hand = model.point("grip_l").map_or(Vec3::new(-0.115, -0.01, -0.34), |g| (grip * g).transform_point3(Vec3::ZERO) + Vec3::new(-0.05, 0.0, 0.0));
        let mut draws = Vec::new();
        if !push_vm_arms(models, base, hand, VM_GRIP, p, livery) { push_vm_support(&mut draws, models, base, hand, p, livery); }
        return draws;
    }
    // A view model built from parts (`registry::Part`), in the hand.
    if let Some(parts) = peakrunner_core::registry::registry().weapons.get(shown as usize).map(|w| &w.parts).filter(|parts| !parts.is_empty()) {
        let (mut draws, mut glow) = (Vec::new(), Vec::new());
        push_parts(&mut draws, &mut glow, base, parts, livery.accent);
        if !push_vm_arms(models, base, vm_hand(look), VM_GRIP, p, livery) { push_vm_support(&mut draws, models, base, vm_hand(look), p, livery); }
        return draws;
    }
    use peakrunner_core::registry::View;
    let (frame, mut draws) = match view {
        View::Disc => {
            // Pivot around the muzzle, not the grip: move the rear toward the
            // right edge without moving the launch point or changing ballistics.
            let angled = base * Mat4::from_translation(VM_MUZZLE_DISC)
                * Mat4::from_rotation_y(0.12)
                * Mat4::from_translation(-VM_MUZZLE_DISC);
            (angled, disc_launcher(angled, world.time, cooldown, shot_age, anim.ready_flash, loaded, team))
        }
        View::Chaingun => (base, chaingun(base, anim, shot_age, team)),
        View::Railgun => (base, railgun(base, cooldown, shot_age, loaded, team)),
        View::Rifle => (base, laser_rifle(base, world.time, cooldown, shot_age, loaded, team)),
        View::Plasma => (base, plasma_gun(base, world.time, cooldown, shot_age, loaded, team)),
        View::Blaster => (base, blaster(base, world.time, shot_age, loaded, team)),
        View::Launcher | View::Mortar => (base, grenade_launcher(base, anim, cooldown, shot_age, loaded, team)),
    };
    if !push_vm_arms(models, frame, vm_hand(look), VM_GRIP, p, livery) { push_vm_support(&mut draws, models, frame, vm_hand(look), p, livery); }
    // The mortar: the launcher, but a bigger heavier tube.
    if view == View::Mortar {
        let pivot = frame.w_axis.truncate();
        let grow = Mat4::from_translation(pivot) * Mat4::from_scale(Vec3::new(1.3, 1.3, 1.15)) * Mat4::from_translation(-pivot);
        for d in &mut draws { d.model = grow * d.model; d.color *= 0.8; }
    }
    draws
}

/// Whether any of player `i`'s body (head, chest or hips) can be seen from
/// `eye` through the map.
pub(crate) fn body_in_sight(world: &World, eye: Vec3, i: usize) -> bool {
    let at = drawn_at(world, i);
    [1.6f32, 0.9, 0.2].iter().any(|h| world.sight_clear(eye, at + Vec3::Y * *h))
}

/// First person in a blade zone: the blade in your hands, with its glow and
/// a cut's smear, drawn with the world (the view's pass has no glow) at the
/// camera, and your arms on its hilt.
fn push_first_person_blade(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, models: &mut Vec<ModelDraw>, world: &World, view: Mat4, anim: &ViewAnim) {
    if world.state == MatchState::Flyby || world.spectating || third_person_view(world) { return; }
    let Some(p) = world.players.get(world.player_id) else { return };
    if !p.alive || world.blade_zone_of(world.player_id).is_none() { return; }
    // Drawn between steps, as the camera is.
    let sub = (world.acc / peakrunner_core::sim::STEP).clamp(0.0, 1.0);
    let bob = anim.bob_offset(world.time) * 0.6;
    let held = crate::blade_look::view_frame(crate::blade_look::pose(&p.blade, sub), bob);
    let to_world = view.inverse();
    let trail: Vec<Mat4> = crate::blade_look::trail(&p.blade, sub, 12).into_iter().map(|q| to_world * crate::blade_look::view_frame(q, bob)).collect();
    let heat = if p.blade.swing == peakrunner_core::sim::blade::POWER || p.blade.charging() { p.blade.charge() } else { 0.0 };
    let who = if p.net_id != 0 { p.net_id } else { world.player_id as u32 + 1 };
    crate::blade_look::push(lit, emit, to_world * held, crate::blade_look::color(who), heat, &trail, true);
    let livery = crate::player_model::livery(world, world.player_id);
    let _ = push_vm_arms_in(models, to_world, held, crate::blade_look::LEFT_HAND, crate::blade_look::RIGHT_HAND, p, livery);
}

/// The plasma gun's bolt color: hot orange.
pub(crate) const PLASMA_GLOW: Vec3 = Vec3::new(1.0, 0.52, 0.14);
/// The blaster's bolt color: electric violet.
pub(crate) const BLASTER_GLOW: Vec3 = Vec3::new(0.74, 0.44, 1.0);

/// Plasma gun: a chunky gunmetal receiver around a caged glass chamber whose
/// orange core swells as the next bolt charges, a short wide barrel with
/// copper heat fins, and a coil at the back.
fn plasma_gun(base: Mat4, time: f32, cooldown: f32, shot_age: f32, loaded: bool, team: Vec3) -> Vec<LitDraw> {
    let body = Vec3::new(0.30, 0.31, 0.33);
    let dark = Vec3::new(0.07, 0.075, 0.085);
    let copper = Vec3::new(0.66, 0.38, 0.18);
    let charge = if loaded { 1.0 - (cooldown / peakrunner_core::defs::weapons().plasma.reload).clamp(0.0, 1.0) } else { 0.0 };
    let mut draws = Vec::with_capacity(24);
    let mut put = |mesh, model: Mat4, color: Vec3, emit: f32| draws.push(LitDraw { mesh, model: base * model, color, emit, mode: 0.0 });
    let at = |p: Vec3, s: Vec3| Mat4::from_translation(p) * Mat4::from_scale(s);
    let along = |p: Vec3, r: f32, len: f32| Mat4::from_translation(p) * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2) * Mat4::from_scale(Vec3::new(r, len, r));
    put(MeshId::Bevel, at(Vec3::new(0.0, -0.01, 0.14), Vec3::new(0.15, 0.15, 0.32)), body, 0.0);
    put(MeshId::Bevel, at(Vec3::new(0.0, -0.14, 0.22), Vec3::new(0.07, 0.17, 0.1)), dark, 0.0);
    // The chamber on top: a glass capsule whose orange core swells as
    // the next bolt charges, held by dark clamps.
    let swell = 1.0 + 0.25 * charge + 0.04 * (time * 9.0).sin();
    put(MeshId::Sphere, at(Vec3::new(0.0, 0.1, -0.02), Vec3::new(0.05, 0.045, 0.15) * swell), PLASMA_GLOW, 0.3 + 0.8 * charge);
    put(MeshId::Bevel, at(Vec3::new(0.0, 0.065, -0.02), Vec3::new(0.12, 0.03, 0.3)), dark, 0.0);
    for k in 0..3 {
        put(MeshId::Cube, at(Vec3::new(0.0, 0.105, 0.08 - k as f32 * 0.1), Vec3::new(0.115, 0.02, 0.022)), dark, 0.0);
    }
    // Barrel and two copper heat rings.
    put(MeshId::Disc, along(Vec3::new(0.0, 0.01, -0.34), 0.065, 0.36), body, 0.0);
    for k in 0..2 {
        put(MeshId::Disc, along(Vec3::new(0.0, 0.01, -0.27 - k as f32 * 0.12), 0.08, 0.018), copper * 0.7, 0.0);
    }
    put(MeshId::Disc, along(Vec3::new(0.0, 0.01, -0.53), 0.085, 0.03), dark, 0.0);
    // The coil at the back, and the team stripe.
    put(MeshId::Disc, along(Vec3::new(0.0, 0.03, 0.33), 0.06, 0.08), copper, 0.0);
    put(MeshId::Cube, at(Vec3::new(0.077, -0.01, 0.14), Vec3::new(0.004, 0.035, 0.26)), team, 0.35);
    let flash = (-shot_age * 16.0).exp();
    if flash > 0.02 {
        put(MeshId::Sphere, at(Vec3::new(0.0, 0.01, -0.6), Vec3::splat(0.05 + 0.08 * flash)), PLASMA_GLOW, 2.2 * flash);
    }
    draws
}

/// Blaster: a compact pale sidearm with a violet energy cell along its side
/// and a ringed emitter tip that flares with each bolt.
fn blaster(base: Mat4, time: f32, shot_age: f32, loaded: bool, team: Vec3) -> Vec<LitDraw> {
    let body = Vec3::new(0.78, 0.80, 0.84);
    let dark = Vec3::new(0.08, 0.085, 0.1);
    let mut draws = Vec::with_capacity(16);
    let mut put = |mesh, model: Mat4, color: Vec3, emit: f32| draws.push(LitDraw { mesh, model: base * model, color, emit, mode: 0.0 });
    let at = |p: Vec3, s: Vec3| Mat4::from_translation(p) * Mat4::from_scale(s);
    let along = |p: Vec3, r: f32, len: f32| Mat4::from_translation(p) * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2) * Mat4::from_scale(Vec3::new(r, len, r));
    put(MeshId::Bevel, at(Vec3::new(0.0, 0.0, 0.06), Vec3::new(0.11, 0.12, 0.36)), body, 0.0);
    put(MeshId::Bevel, Mat4::from_translation(Vec3::new(0.0, -0.13, 0.16)) * Mat4::from_rotation_x(0.3) * Mat4::from_scale(Vec3::new(0.065, 0.18, 0.09)), dark, 0.0);
    put(MeshId::Bevel, at(Vec3::new(0.0, 0.01, -0.22), Vec3::new(0.075, 0.08, 0.24)), body, 0.0);
    // The energy cell along the top, dark in an empty tank.
    let cell = if loaded { 0.6 + 0.25 * (time * 5.0).sin().abs() } else { 0.05 };
    put(MeshId::Bevel, at(Vec3::new(0.0, 0.07, -0.04), Vec3::new(0.06, 0.03, 0.34)), dark, 0.0);
    put(MeshId::Cube, at(Vec3::new(0.0, 0.088, -0.04), Vec3::new(0.026, 0.012, 0.3)), BLASTER_GLOW, cell);
    // The emitter: a dark collar and two prongs either side of the bore.
    put(MeshId::Disc, along(Vec3::new(0.0, 0.01, -0.37), 0.05, 0.06), dark, 0.0);
    let flash = (-shot_age * 24.0).exp();
    for s in [-1.0f32, 1.0] {
        put(MeshId::Cube, at(Vec3::new(s * 0.032, 0.01, -0.44), Vec3::new(0.014, 0.05, 0.1)), dark, 0.0);
    }
    put(MeshId::Disc, along(Vec3::new(0.0, 0.01, -0.41), 0.03, 0.015), BLASTER_GLOW, 0.4 + 1.6 * flash);
    put(MeshId::Cube, at(Vec3::new(-0.057, 0.0, 0.06), Vec3::new(0.004, 0.03, 0.24)), team, 0.35);
    if flash > 0.02 {
        put(MeshId::Sphere, at(Vec3::new(0.0, 0.01, -0.47), Vec3::splat(0.03 + 0.05 * flash)), BLASTER_GLOW, 2.0 * flash);
    }
    draws
}

/// Laser rifle: a long slim receiver and barrel with a glass emitter rod
/// down its top that charges up over the reload and dims when out of energy.
fn laser_rifle(base: Mat4, time: f32, cooldown: f32, shot_age: f32, loaded: bool, team: Vec3) -> Vec<LitDraw> {
    let body = Vec3::new(0.84, 0.87, 0.9);
    let dark = Vec3::new(0.08, 0.09, 0.1);
    let cyan = Vec3::new(0.35, 0.95, 1.0);
    let charge = if loaded { 1.0-(cooldown/peakrunner_core::defs::weapons().laser.reload).clamp(0.0, 1.0) } else { 0.0 };
    let mut draws = Vec::with_capacity(20);
    let mut put = |mesh, at: Vec3, size: Vec3, color: Vec3, emit: f32| draws.push(LitDraw { mesh,
        model: base*Mat4::from_translation(at)*Mat4::from_scale(size), color, emit, mode: 0.0 });
    put(MeshId::Bevel, Vec3::new(0.0, -0.01, 0.12), Vec3::new(0.13, 0.15, 0.36), body, 0.0);
    put(MeshId::Bevel, Vec3::new(0.0, -0.13, 0.22), Vec3::new(0.07, 0.17, 0.1), dark, 0.0);
    put(MeshId::Bevel, Vec3::new(0.0, -0.03, 0.36), Vec3::new(0.08, 0.12, 0.16), dark, 0.0);
    put(MeshId::Bevel, Vec3::new(0.0, 0.0, -0.35), Vec3::new(0.07, 0.07, 0.62), body, 0.0);
    put(MeshId::Cube, Vec3::new(0.0, 0.052, -0.2), Vec3::new(0.03, 0.02, 0.8), cyan, 0.3+charge*1.2);
    for k in 0..4 {
        let z = -0.1-k as f32*0.14;
        put(MeshId::Disc, Vec3::new(0.0, 0.0, z), Vec3::new(0.055, 0.02, 0.055), dark, 0.0);
    }
    put(MeshId::Bevel, Vec3::new(0.0, 0.1, 0.02), Vec3::new(0.05, 0.05, 0.24), dark, 0.0);
    put(MeshId::Cube, Vec3::new(0.066, -0.01, 0.12), Vec3::new(0.004, 0.035, 0.3), team, 0.35);
    let flash = (-shot_age*18.0).exp();
    put(MeshId::Sphere, Vec3::new(0.0, 0.0, -0.68), Vec3::splat(0.04+0.08*flash), cyan, 0.4+2.0*flash+0.2*(time*6.0).sin().abs()*charge);
    draws
}

/// Railgun: a heavy boxy stock with twin conductor rails around the bore,
/// lit blue along their length when charged.
fn railgun(base: Mat4, cooldown: f32, shot_age: f32, loaded: bool, team: Vec3) -> Vec<LitDraw> {
    let steel = Vec3::new(0.34, 0.37, 0.41);
    let dark = Vec3::new(0.07, 0.08, 0.1);
    let blue = Vec3::new(0.35, 0.65, 1.0);
    let charge = if loaded { 1.0-(cooldown/peakrunner_core::defs::weapons().railgun.reload).clamp(0.0, 1.0) } else { 0.0 };
    let mut draws = Vec::with_capacity(20);
    let mut put = |mesh, at: Vec3, size: Vec3, color: Vec3, emit: f32| draws.push(LitDraw { mesh,
        model: base*Mat4::from_translation(at)*Mat4::from_scale(size), color, emit, mode: 0.0 });
    put(MeshId::Bevel, Vec3::new(0.0, -0.02, 0.14), Vec3::new(0.2, 0.2, 0.42), steel, 0.0);
    put(MeshId::Bevel, Vec3::new(0.0, -0.16, 0.22), Vec3::new(0.08, 0.18, 0.12), dark, 0.0);
    put(MeshId::Bevel, Vec3::new(0.0, -0.1, -0.08), Vec3::new(0.12, 0.08, 0.2), dark, 0.0);
    for side in [-1.0, 1.0] {
        put(MeshId::Bevel, Vec3::new(side*0.055, 0.0, -0.36), Vec3::new(0.04, 0.12, 0.66), steel, 0.0);
        put(MeshId::Cube, Vec3::new(side*0.032, 0.0, -0.36), Vec3::new(0.008, 0.07, 0.62), blue, 0.2+1.4*charge);
    }
    for z in [-0.18, -0.42, -0.62] { put(MeshId::Bevel, Vec3::new(0.0, 0.07, z), Vec3::new(0.16, 0.03, 0.05), dark, 0.0); }
    put(MeshId::Cube, Vec3::new(0.101, -0.02, 0.14), Vec3::new(0.004, 0.05, 0.34), team, 0.35);
    let flash = (-shot_age*14.0).exp();
    if flash > 0.02 {
        put(MeshId::Sphere, Vec3::new(0.0, 0.0, -0.72), Vec3::new(0.06, 0.06, 0.12)*(1.0+2.0*flash), blue, 2.2*flash);
    }
    draws
}

/// Where the support hand holds a built-in view (the disc launcher, a bolt
/// weapon or another): the left side of its barrel or rail. The weapon sits
/// at the lower-right screen edge, so this side faces the player; the firing
/// hand on the grip is always off-screen and not drawn.
fn vm_hand(weapon: u8) -> Vec3 {
    // Left-side grip point in the weapon's frame, by built-in view.
    match weapon {
        0 => Vec3::new(-0.30, -0.02, -0.30),
        1 => Vec3::new(-0.135, -0.01, -0.22),
        _ => Vec3::new(-0.115, -0.01, -0.34),
    }
}

/// The support arm at `hand` (the weapon's frame): the body's armor's own
/// when it has them, its `grip_l` glove wrapped round the weapon there and
/// its `forearm_l` up from the wrist; else the built-in hand and sleeve
/// (under the armor's forearm when only that is authored).
fn push_vm_support(draws: &mut Vec<LitDraw>, models: &mut Vec<ModelDraw>, frame: Mat4, hand: Vec3, p: &crate::sim::Player, livery: crate::player_model::Livery) {
    let kit = crate::player_model::armor_model(p.armor.def().build);
    let part = |name: &str| kit.and_then(|(id, k)| k.parts.iter().position(|q| q.name == name).map(|n| (id, n as u16)));
    let (forearm, grip) = (part("forearm_l"), part("grip_l"));
    match grip {
        Some((id, k)) => models.push(ModelDraw { id, part: k, model: frame * Mat4::from_translation(hand), team: livery.color, glow: 0.6, paint: false, plates: livery.plates, shine: livery.shine, skin: RIGID }),
        None => push_vm_hand_at(draws, frame, hand, livery.accent, livery.color, forearm.is_none()),
    }
    if let Some((id, k)) = forearm {
        let (wrist, elbow) = vm_forearm(hand);
        // With the armor's glove, the bone at its rest length so the
        // gauntlet runs down to the glove's cuff.
        let elbow = if grip.is_some() { wrist + (elbow - wrist).normalize() * crate::player_model::FOREARM } else { elbow };
        let bone = crate::player_model::limb_frame(elbow, wrist, Vec3::X, crate::player_model::FOREARM);
        models.push(ModelDraw { id, part: k, model: frame * bone, team: livery.color, glow: 0.6, paint: false, plates: livery.plates, shine: livery.shine, skin: RIGID });
    }
}

/// Where the ball you carry sits in first person, low on the right, and
/// how big it's drawn there (the case is a hand's width across).
const VM_BALL: Vec3 = Vec3::new(0.20, -0.20, -0.56);
const VM_BALL_SCALE: f32 = 0.36;

/// The ball you carry in first person (`frame`: its middle, unscaled),
/// resting on your armor's right forearm, in from the lower right.
fn push_vm_ball(draws: &mut Vec<LitDraw>, models: &mut Vec<ModelDraw>, frame: Mat4, p: &crate::sim::Player, livery: crate::player_model::Livery) {
    draws.extend(ball_draws(frame * Mat4::from_scale(Vec3::splat(VM_BALL_SCALE)) * Mat4::from_translation(Vec3::new(0.0, -0.15, 0.0))));
    let Some((id, kit)) = crate::player_model::armor_model(p.armor.def().build) else { return };
    let part = |name: &str| kit.parts.iter().position(|q| q.name == name).map(|n| n as u16);
    // The wrist under the ball; the elbow down and out of view.
    let wrist = frame.transform_point3(Vec3::new(-0.02, -0.075, -0.02));
    let elbow = wrist + Vec3::new(0.12, -0.22, 0.20).normalize() * crate::player_model::FOREARM;
    let fore = crate::player_model::limb_frame(elbow, wrist, Vec3::X, crate::player_model::FOREARM);
    if let Some(k) = part("forearm_r") {
        models.push(ModelDraw { id, part: k, model: fore, team: livery.color, glow: 0.6, paint: false, plates: livery.plates, shine: livery.shine, skin: RIGID });
    }
}

/// Where each first-person arm comes from, relative to its wrist in view
/// space: the shoulders sit off the lower screen edge, so the forearms run
/// in from the lower corners and the upper arms barely show.
const VM_SHOULDER_L: Vec3 = Vec3::new(-0.40, -0.26, 0.34);
const VM_SHOULDER_R: Vec3 = Vec3::new(0.14, -0.44, 0.30);

/// Both arms on the weapon in first person, from the body's armor: its grip
/// gloves (`grip_l` on the support side at `left`, `grip_r` round the pistol
/// grip at `right`, both in the weapon's frame `frame`), each forearm from
/// the glove's wrist point to an elbow found by two-bone reach from a
/// shoulder off the screen edge, and the upper arm above it. False when the
/// armor has no grip gloves (the caller draws the built-in support hand).
fn push_vm_arms(models: &mut Vec<ModelDraw>, frame: Mat4, left: Vec3, right: Vec3, p: &crate::sim::Player, livery: crate::player_model::Livery) -> bool {
    push_vm_arms_in(models, Mat4::IDENTITY, frame, left, right, p, livery)
}

/// `push_vm_arms`, the view carried into the world by `out` (the blade's
/// arms, drawn with the world: `push_first_person_blade`).
fn push_vm_arms_in(models: &mut Vec<ModelDraw>, out: Mat4, frame: Mat4, left: Vec3, right: Vec3, p: &crate::sim::Player, livery: crate::player_model::Livery) -> bool {
    use crate::player_model::{ik_elbow, limb_frame, FOREARM, UPPER_ARM};
    let Some((id, kit)) = crate::player_model::armor_model(p.armor.def().build) else { return false };
    let part = |name: &str| kit.parts.iter().position(|q| q.name == name).map(|n| n as u16);
    let arms = [(part("grip_l"), kit.point("wrist_grip_l"), part("forearm_l"), part("upper_arm_l"), left, VM_SHOULDER_L, -1.0),
                (part("grip_r"), kit.point("wrist_grip_r"), part("forearm_r"), part("upper_arm_r"), right, VM_SHOULDER_R, 1.0)];
    if arms.iter().any(|a| a.0.is_none() || a.1.is_none()) { return false; }
    for (glove, wrist_at, forearm, upper, at, shoulder, side) in arms {
        let glove_frame = frame * Mat4::from_translation(at);
        models.push(ModelDraw { id, part: glove.unwrap(), model: out * glove_frame, team: livery.color, glow: 0.6, paint: false, plates: livery.plates, shine: livery.shine, skin: RIGID });
        let wrist = (glove_frame * wrist_at.unwrap()).transform_point3(Vec3::ZERO);
        let shoulder = wrist + shoulder;
        let elbow = ik_elbow(shoulder, wrist, UPPER_ARM, FOREARM, Vec3::new(side, -0.8, 0.2));
        // The armor's plates, over the undersuit's sleeve (the third-person
        // body wears a skinned undersuit, which first person doesn't draw).
        let tag = if side < 0.0 { "l" } else { "r" };
        let (fore, top) = (limb_frame(elbow, wrist, Vec3::X, FOREARM), limb_frame(shoulder, elbow, Vec3::X, UPPER_ARM));
        for (k, bone) in [(forearm, fore), (upper, top), (part(&format!("sleeve_fore_{tag}")), fore), (part(&format!("sleeve_upper_{tag}")), top)] {
            if let Some(k) = k {
                models.push(ModelDraw { id, part: k, model: out * bone, team: livery.color, glow: 0.6, paint: false, plates: livery.plates, shine: livery.shine, skin: RIGID });
            }
        }
    }
    true
}

/// The support forearm's wrist and elbow for a hand at `hand`, in the
/// weapon's frame: the elbow off the lower-left of the screen.
fn vm_forearm(hand: Vec3) -> (Vec3, Vec3) {
    (hand + Vec3::new(-0.02, -0.05, 0.10), hand + Vec3::new(-0.36, -0.36, 0.46))
}

/// The built-in support hand at `hand` in the weapon's frame (a gloved left
/// hand wrapped round the side, its forearm running in from the lower left);
/// `vambrace`: its forearm wears the built-in plate (not when the armor's
/// forearm is drawn).
fn push_vm_hand_at(draws: &mut Vec<LitDraw>, frame: Mat4, hand: Vec3, accent: Vec3, plate: Vec3, vambrace: bool) {
    // The third-person armor's arm (player_model): a leather glove with a
    // steel knuckle plate, the dark undersuit sleeve, a team-coloured
    // vambrace and a steel cuff at the wrist.

    let leather = Vec3::new(0.12, 0.10, 0.09);
    let steel = Vec3::new(0.36, 0.40, 0.45);
    let suit = Vec3::new(0.085, 0.09, 0.10);
    let mut part = |mesh, model: Mat4, color: Vec3, glow: f32| {
        draws.push(LitDraw { mesh, model: frame * model, color, emit: glow, mode: 0.0 });
    };
    // Palm against the weapon, fingers curling over the top, knuckle plate.
    part(MeshId::Bevel, Mat4::from_translation(hand) * Mat4::from_scale(Vec3::new(0.08, 0.12, 0.15)), leather, 0.0);
    part(MeshId::Bevel, Mat4::from_translation(hand + Vec3::new(0.045, 0.075, 0.0))
        * Mat4::from_rotation_z(-0.5) * Mat4::from_scale(Vec3::new(0.10, 0.045, 0.14)), leather, 0.0);
    part(MeshId::Bevel, Mat4::from_translation(hand + Vec3::new(-0.045, 0.01, 0.0))
        * Mat4::from_scale(Vec3::new(0.03, 0.10, 0.13)), steel, 0.0);
    part(MeshId::Cube, Mat4::from_translation(hand + Vec3::new(-0.062, 0.01, 0.0))
        * Mat4::from_scale(Vec3::new(0.004, 0.05, 0.08)), accent, 0.35);
    // Forearm to an elbow off the lower-left of the screen.
    let (wrist, elbow) = vm_forearm(hand);
    let axis = elbow - wrist;
    let length = axis.length();
    let arm = Mat4::from_translation((wrist + elbow) * 0.5)
        * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::Y, axis.normalize()));
    part(MeshId::Capsule, arm * Mat4::from_scale(Vec3::new(0.11, length + 0.06, 0.115)), suit, 0.0);
    if !vambrace { return; }
    part(MeshId::Armor, arm * Mat4::from_translation(Vec3::new(0.0, length * 0.05, 0.0))
        * Mat4::from_scale(Vec3::new(0.14, length * 0.62, 0.145)), plate, 0.0);
    part(MeshId::Armor, arm * Mat4::from_translation(Vec3::new(0.0, -length * 0.42, 0.0))
        * Mat4::from_scale(Vec3::new(0.125, 0.045, 0.13)), steel, 0.0);
    part(MeshId::Cube, arm * Mat4::from_translation(Vec3::new(0.0, length * 0.2, -0.073))
        * Mat4::from_scale(Vec3::new(0.05, 0.03, 0.02)), Vec3::new(0.32, 0.88, 1.0), 0.6);
}

fn along_z(at: Vec3, radius: f32, length: f32) -> Mat4 {
    Mat4::from_translation(at) * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)
        * Mat4::from_scale(Vec3::new(radius, length, radius))
}

/// Rotary chaingun: six barrels in a shroud ring and a vented heat jacket,
/// a rear motor housing and a side ammo drum. The barrels spin up and coast
/// down; the jacket vents glow with (cosmetic) heat.
fn chaingun(base: Mat4, anim: &ViewAnim, shot_age: f32, team: Vec3) -> Vec<LitDraw> {
    let gun = Vec3::new(0.25, 0.28, 0.31);
    let dark = Vec3::new(0.07, 0.08, 0.10);
    let steel = Vec3::new(0.44, 0.48, 0.52);
    let olive = Vec3::new(0.22, 0.25, 0.19);
    let amber = Vec3::new(1.0, 0.55, 0.16);
    let mut draws = Vec::with_capacity(48);
    let mut put = |mesh, model: Mat4, color: Vec3, glow: f32| draws.push(LitDraw { mesh, model: base * model, color, emit: glow, mode: 0.0 });
    let box_at = |at: Vec3, size: Vec3| Mat4::from_translation(at) * Mat4::from_scale(size);
    // Receiver, carry rail and grip.
    put(MeshId::Bevel, box_at(Vec3::new(0.0, -0.01, 0.07), Vec3::new(0.17, 0.15, 0.32)), gun, 0.0);
    put(MeshId::Cube, box_at(Vec3::new(0.0, 0.092, 0.03), Vec3::new(0.05, 0.022, 0.28)), steel, 0.0);
    put(MeshId::Bevel, Mat4::from_translation(Vec3::new(0.0, -0.13, 0.12)) * Mat4::from_rotation_x(0.25)
        * Mat4::from_scale(Vec3::new(0.07, 0.17, 0.10)), dark, 0.0);
    put(MeshId::Cube, box_at(Vec3::new(0.087, -0.005, 0.07), Vec3::new(0.004, 0.04, 0.26)), team, 0.35);
    // Rear motor housing with a status ring.
    put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, 0.24), 0.085, 0.13), dark, 0.0);
    put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, 0.305), 0.06, 0.012), amber, 0.25 + anim.heat * 0.9);
    // Side drum turning with the feed, brass rounds on its rim.
    let drum = Mat4::from_translation(Vec3::new(-0.155, -0.045, 0.03))
        * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2) * Mat4::from_rotation_y(anim.spin * 0.12);
    put(MeshId::Disc, drum * Mat4::from_scale(Vec3::new(0.09, 0.075, 0.09)), olive, 0.0);
    for k in 0..6 {
        let a = k as f32 * std::f32::consts::TAU / 6.0;
        put(MeshId::Cube, drum * Mat4::from_translation(Vec3::new(a.cos() * 0.07, 0.04, a.sin() * 0.07))
            * Mat4::from_scale(Vec3::splat(0.018)), Vec3::new(0.82, 0.62, 0.26), 0.12);
    }
    put(MeshId::Cube, box_at(Vec3::new(-0.1, -0.03, 0.03), Vec3::new(0.05, 0.03, 0.07)), dark, 0.0);
    // Barrel cluster: hub, clamp rings, spinning barrels, bores.
    put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.29), 0.028, 0.56), dark, 0.0);
    put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.09), 0.098, 0.05), steel, 0.0);
    put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.31), 0.094, 0.025), dark, 0.0);
    put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.53), 0.104, 0.035), steel, 0.0);
    for k in 0..6 {
        let a = anim.spin + k as f32 * std::f32::consts::TAU / 6.0;
        let o = Vec3::new(a.cos(), a.sin(), 0.0) * 0.062;
        put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.30) + o, 0.024, 0.54), Vec3::new(0.32, 0.35, 0.38), 0.0);
        put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.572) + o, 0.017, 0.006), Vec3::splat(0.02), 0.0);
    }
    // Vented heat jacket: fixed slats that glow amber as the gun heats.
    let vent = dark + (amber - dark) * anim.heat;
    for k in 0..6 {
        let a = (k as f32 + 0.5) * std::f32::consts::TAU / 6.0;
        let o = Vec3::new(a.cos(), a.sin(), 0.0) * 0.1;
        put(MeshId::Cube, Mat4::from_translation(Vec3::new(0.0, 0.02, -0.31) + o) * Mat4::from_rotation_z(a)
            * Mat4::from_scale(Vec3::new(0.014, 0.024, 0.34)), vent, anim.heat * 1.1);
    }
    // A small orange-red burst on each round (the grenade launcher's blast,
    // much smaller), turned a little every shot.
    if shot_age < 0.035 {
        let k = 1.0 - shot_age / 0.035;
        let fire = Vec3::new(1.0, 0.36, 0.1);
        // Just past the barrels, long enough to clear the shroud.
        let spin = Mat4::from_translation(Vec3::new(0.0, 0.02, -0.66)) * Mat4::from_rotation_z(shot_age * 900.0);
        put(MeshId::Sphere, spin * Mat4::from_scale(Vec3::new(0.14, 0.024, 0.035) * (0.75 + 0.25 * k)), fire, 1.8);
        put(MeshId::Sphere, spin * Mat4::from_scale(Vec3::new(0.024, 0.14, 0.035) * (0.75 + 0.25 * k)), fire, 1.8);
        put(MeshId::Sphere, spin * Mat4::from_translation(Vec3::new(0.0, 0.0, -0.03)) * Mat4::from_scale(Vec3::splat(0.045 + 0.02 * k)),
            Vec3::new(1.0, 0.86, 0.55), 2.0);
    }
    draws
}

/// Grenade launcher: a wide bore, a four-chamber revolving cylinder with
/// round lights, a pump grip and a stock. The existing reload timer drives a
/// cylinder step and a pump rack; a fresh round rotates up as it completes.
fn grenade_launcher(base: Mat4, anim: &ViewAnim, cooldown: f32, shot_age: f32, loaded: bool, team: Vec3) -> Vec<LitDraw> {
    let gun = Vec3::new(0.23, 0.25, 0.27);
    let dark = Vec3::new(0.07, 0.075, 0.08);
    let steel = Vec3::new(0.44, 0.47, 0.50);
    let olive = Vec3::new(0.30, 0.34, 0.19);
    let amber = Vec3::new(1.0, 0.62, 0.18);
    let reload = weapon_reload(2);
    let progress = if cooldown > 0.0 { 1.0 - cooldown / reload } else { 1.0 };
    // The cylinder turns one step over the middle of the reload.
    let step = ((progress - 0.2) / 0.6).clamp(0.0, 1.0);
    let turn = step * step * (3.0 - 2.0 * step);
    let steps = anim.cylinder as f32 - if cooldown > 0.0 { 1.0 - turn } else { 0.0 };
    let angle = steps * std::f32::consts::FRAC_PI_2;
    let pump = (progress * std::f32::consts::PI).sin() * if cooldown > 0.0 { 0.075 } else { 0.0 };
    let mut draws = Vec::with_capacity(32);
    let mut put = |mesh, model: Mat4, color: Vec3, glow: f32| draws.push(LitDraw { mesh, model: base * model, color, emit: glow, mode: 0.0 });
    let box_at = |at: Vec3, size: Vec3| Mat4::from_translation(at) * Mat4::from_scale(size);
    // Receiver, top strap over the cylinder, stock, grip, team stripe.
    put(MeshId::Bevel, box_at(Vec3::new(0.0, -0.01, 0.12), Vec3::new(0.17, 0.16, 0.24)), gun, 0.0);
    put(MeshId::Cube, box_at(Vec3::new(0.0, 0.07, -0.1), Vec3::new(0.05, 0.022, 0.30)), steel, 0.0);
    put(MeshId::Bevel, box_at(Vec3::new(0.0, -0.04, 0.3), Vec3::new(0.08, 0.13, 0.16)), olive, 0.0);
    put(MeshId::Bevel, Mat4::from_translation(Vec3::new(0.0, -0.14, 0.15)) * Mat4::from_rotation_x(0.25)
        * Mat4::from_scale(Vec3::new(0.07, 0.16, 0.10)), dark, 0.0);
    put(MeshId::Cube, box_at(Vec3::new(0.087, -0.01, 0.12), Vec3::new(0.004, 0.04, 0.2)), team, 0.35);
    // Ready light on the sight.
    put(MeshId::Cube, box_at(Vec3::new(0.0, 0.092, 0.02), Vec3::new(0.03, 0.018, 0.03)),
        if cooldown > 0.0 || !loaded { dark } else { amber }, if cooldown > 0.0 || !loaded { 0.0 } else { 0.9 });
    // Wide-bore barrel with a muzzle ring.
    put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.40), 0.075, 0.38), gun, 0.0);
    put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.575), 0.088, 0.035), steel, 0.0);
    put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.594), 0.058, 0.004), Vec3::splat(0.02), 0.0);
    // Pump grip racks back and forward during the reload.
    put(MeshId::Bevel, box_at(Vec3::new(0.0, -0.075, -0.36 + pump), Vec3::new(0.10, 0.06, 0.16)), Vec3::splat(0.11), 0.0);
    // Revolving cylinder: four chambers, the fired one dark until it cycles.
    let cyl = Mat4::from_translation(Vec3::new(0.0, -0.035, -0.09)) * Mat4::from_rotation_z(angle);
    put(MeshId::Disc, cyl * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2) * Mat4::from_scale(Vec3::new(0.112, 0.17, 0.112)), olive, 0.0);
    // Chamber k is on top after the cylinder has stepped n times when
    // k + n = 0 (mod 4). The chamber that just fired stays dark as it turns
    // away; the fresh round brightens as it rotates up into place.
    let steps_done = anim.cylinder % 4;
    let fired_chamber = (4 - (steps_done + 3) % 4) % 4;
    let arriving = (4 - steps_done) % 4;
    for k in 0..4u32 {
        let a = k as f32 * std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_2;
        let at = Vec3::new(a.cos() * 0.066, a.sin() * 0.066, 0.0);
        put(MeshId::Disc, cyl * along_z(at, 0.03, 0.172), dark, 0.0);
        let reloading = cooldown > 0.0;
        let glow = if reloading && k == fired_chamber { 0.0 }
            else if reloading && k == arriving { 0.2 + 0.7 * turn } else { 0.8 };
        // An empty launcher has no rounds to light.
        let glow = if loaded { glow } else { 0.0 };
        // The round light sits on the cylinder's outer face over its chamber,
        // so the player sees rounds turn past.
        let out = Vec3::new(a.cos(), a.sin(), 0.0);
        put(MeshId::Cube, cyl * Mat4::from_translation(out * 0.114 + Vec3::new(0.0, 0.0, 0.03)) * Mat4::from_rotation_z(a)
            * Mat4::from_scale(Vec3::new(0.008, 0.03, 0.06)), if glow > 0.0 { amber } else { dark }, glow);
    }
    // A small orange-red explosion throws the round out of the bore: a hot
    // core, flame tongues round the muzzle, and the fireball rolling after
    // the round as it shrinks away.
    if shot_age < 0.09 {
        let k = 1.0 - shot_age / 0.09;
        let fire = Vec3::new(1.0, 0.34, 0.08);
        let out = shot_age / 0.09;
        if shot_age < 0.05 {
            put(MeshId::Sphere, box_at(Vec3::new(0.0, 0.02, -0.63), Vec3::splat(0.05 + 0.03 * k)), Vec3::new(1.0, 0.88, 0.58), 2.1);
            for n in 0..4 {
                let a = n as f32 * std::f32::consts::FRAC_PI_2 + 0.4;
                put(MeshId::Sphere, Mat4::from_translation(Vec3::new(0.0, 0.02, -0.62)) * Mat4::from_rotation_z(a)
                    * Mat4::from_translation(Vec3::new(0.07 + 0.05 * out, 0.0, 0.0)) * Mat4::from_scale(Vec3::new(0.06, 0.022, 0.03) * k.max(0.4)),
                    Vec3::new(1.0, 0.24, 0.05), 1.8);
            }
        }
        put(MeshId::Sphere, box_at(Vec3::new(0.0, 0.02, -0.68 - 0.25 * out), Vec3::new(0.09, 0.09, 0.12) * (0.5 + 0.8 * k)), fire, 1.6);
    }
    draws
}

pub fn normal_columns(model: Mat4) -> [[f32; 4]; 3] {
    let m = Mat3::from_mat4(model);
    let n = m.inverse().transpose();
    let c = n.to_cols_array();
    [
        [c[0], c[1], c[2], 0.0],
        [c[3], c[4], c[5], 0.0],
        [c[6], c[7], c[8], 0.0],
    ]
}

/// A fixed rocket turret (`TurretWeapon::Rocket`): the library rocket
/// turret's authored model (`models/turrets/rocket-turret/`) seated on the
/// map's static mount and turned to its aim, in its team's color (gray and
/// dim while unpowered), else that deployable's look.
/// A sensor's radar head (`models/equipment/sensor/`): the collar on the
/// mast's top and the antenna turning about it while the sensor is powered
/// (stopped when it isn't, dark when wrecked), in its team's color. The
/// map's kit draws the base and mast.
fn push_sensor_head(models:&mut Vec<ModelDraw>,d:&peakrunner_core::equipment::Definition,
                    s:&peakrunner_core::equipment::State,time:f32) {
    let Some((id,model))=crate::model::get("equipment/sensor") else {return};
    // The mast's top is 1.2 m over the sensor's point (the kit's equipment).
    let top=Mat4::from_translation(d.pos()+Vec3::Y*1.2);
    let phase=d.position[0]*0.13+d.position[2]*0.07;
    let on=s.powered && s.health>0.;
    let yaw=if on {phase+time*1.1} else {phase};
    let team=if d.team==1 {Team::Glacier} else {Team::Ember};
    let (paint,glow)=if on {(crate::player_model::team_color(team),1.0)} else {(Vec3::splat(0.32),0.15)};
    for (k,part) in model.parts.iter().enumerate() {
        let turn=if part.name=="head" {Mat4::from_rotation_y(yaw)} else {Mat4::IDENTITY};
        models.push(ModelDraw {id,part:k as u16,model:top*turn*part.node,team:paint,glow,paint:false,plates:0.0,shine:0.0,skin:RIGID});
    }
}

fn push_rocket_turret(lit:&mut Vec<LitDraw>,emit:&mut Vec<EmitDraw>,models:&mut Vec<ModelDraw>,
                      d:&peakrunner_core::equipment::Definition,s:&peakrunner_core::equipment::State,time:f32) {
    // The mount's top is 0.2 m under the turret's pivot (the kit's equipment).
    let seat=d.pos()-Vec3::Y*0.2;
    let team=if d.team==1 {Team::Glacier} else {Team::Ember};
    let key=peakrunner_core::equipment::ROCKET_TURRET;
    if let Some((id,model))=crate::model::get(&format!("turrets/{key}")) {
        let yaw=(-s.aim.x).atan2(-s.aim.z);
        let (paint,glow)=if s.powered && s.health>0. {(crate::player_model::team_color(team),1.0)} else {(Vec3::splat(0.32),0.25)};
        crate::model::push(models,id,model,Mat4::from_translation(seat)*Mat4::from_rotation_y(yaw),paint,glow,&|_| None);
        return;
    }
    let Some(kind)=crate::sim::deploy::DeployKind::from_key(key) else {return};
    let unit=crate::sim::deploy::Deployable {owner:0,kind,team,pos:seat,yaw:0.,health:kind.def().hull,cooldown:s.cooldown,
        aim:s.aim,age:0.};
    push_deployable(lit,emit,&unit,time);
}

/// Runtime turret head: a housing that yaws with the aim, a cradle that also
/// pitches, and barrels by weapon type. The static mount is baked into the map.
fn push_turret_head(lit:&mut Vec<LitDraw>,emit:&mut Vec<EmitDraw>,d:&peakrunner_core::equipment::Definition,s:&peakrunner_core::equipment::State) {
    let aim=if s.aim.length_squared()>1e-6 {s.aim.normalize()} else {Vec3::NEG_Z};
    let flat=Vec3::new(aim.x,0.,aim.z).normalize_or(Vec3::NEG_Z);
    let yaw=glam::Quat::from_rotation_arc(Vec3::NEG_Z,flat);
    let full=glam::Quat::from_rotation_arc(Vec3::NEG_Z,aim);
    let base=Mat4::from_translation(d.pos());
    let dark=Vec3::new(0.16,0.19,0.22);let steel=Vec3::new(0.30,0.33,0.36);
    let accent=if d.team==0 {Vec3::new(0.78,0.26,0.18)} else {Vec3::new(0.22,0.66,0.78)};
    let part=|lit:&mut Vec<LitDraw>,q:glam::Quat,mesh,at:Vec3,size:Vec3,color:Vec3,glow:f32| lit.push(LitDraw {
        mesh,model:base*Mat4::from_quat(q)*Mat4::from_translation(at)*Mat4::from_scale(size),color,emit:glow,mode:0.});
    // Yaw housing with team stripes.
    part(lit,yaw,MeshId::Bevel,Vec3::new(0.,-0.25,0.2),Vec3::new(2.3,0.9,2.3),steel,0.);
    for x in [-1.16,1.16] {part(lit,yaw,MeshId::Cube,Vec3::new(x,-0.25,0.2),Vec3::new(0.04,0.22,1.9),accent,0.25);}
    // Pitch cradle cheeks and receiver.
    for x in [-0.95,0.95] {part(lit,full,MeshId::Bevel,Vec3::new(x,0.25,0.),Vec3::new(0.32,1.0,1.5),dark,0.);}
    part(lit,full,MeshId::Bevel,Vec3::new(0.,0.25,-0.1),Vec3::new(1.5,0.8,1.7),steel,0.);
    if d.weapon==peakrunner_core::equipment::TurretWeapon::Plasma {
        part(lit,full,MeshId::Cube,Vec3::new(0.,0.25,-1.9),Vec3::new(0.62,0.62,2.4),dark,0.);
        for z in [-1.2,-2.0,-2.8] {
            part(lit,full,MeshId::Cube,Vec3::new(0.,0.25,z),Vec3::new(0.9,0.9,0.14),Vec3::new(0.35,1.0,0.3),if s.powered {0.9} else {0.05});
        }
        if s.powered {
            emit.push(EmitDraw {mesh:MeshId::Sphere,
                model:Mat4::from_translation(d.pos()+aim*3.2)*Mat4::from_scale(Vec3::splat(0.38)),
                color:[0.35,1.,0.12,0.6]});
        }
    } else {
        for x in [-0.42,0.42] {
            part(lit,full,MeshId::Cube,Vec3::new(x,0.25,-2.0),Vec3::new(0.26,0.26,2.8),dark,0.);
            part(lit,full,MeshId::Cube,Vec3::new(x,0.25,-3.35),Vec3::new(0.4,0.4,0.36),steel,0.);
        }
    }
}


#[cfg(test)]
mod projectile_visibility {
    use super::*;

    #[test]
    fn beacons_shift_from_red_to_blue_through_purple() {
        let mut p = peakrunner_core::control::Point { name: "Beacon".into(), pos: Vec3::ZERO, radius: 12.0, drain_radius: 0.0,
            drain_rate: 0.0, active: true, owner: Some(0), progress: 0.0, capturing: None, contested: false,
            turret: None, turret_at: Vec3::ZERO, turret_aim: Vec3::ZERO, turret_cooldown: 0.0, turret_range: 0.0, ..Default::default() };
        let red = beacon_color(&p);
        assert!(red.x > 0.9 && red.z < 0.2);
        p.capturing = Some(1);
        p.progress = 0.5;
        let mid = beacon_color(&p);
        assert!(mid.x > 0.5 && mid.z > 0.8 && mid.y < 0.2, "purple halfway: {mid}");
        p.progress = 1.0;
        let blue = beacon_color(&p);
        assert!(blue.z > 0.9 && blue.x < 0.2);
        // A neutral point goes straight from white toward the capturer.
        p.owner = None; p.progress = 0.5;
        let half = beacon_color(&p);
        assert!(half.z > half.x);
    }

    /// Brightness a lit draw keeps even facing away from the sun: its
    /// self-lit term, which the shader adds unfogged by lighting.
    fn self_lit_luma(d: &LitDraw) -> f32 {
        (0.3 * d.color.x + 0.59 * d.color.y + 0.11 * d.color.z) * d.emit
    }

    /// Every round in flight must read as a bright streak from any side:
    /// no near-black part, no degenerate transform, and additive glow only.
    /// A dark hub on the disc once showed incoming fire as black lines.
    #[test]
    fn projectile_draws_never_render_dark() {
        let dirs: Vec<Vec3> = (0..400).map(|i| {
            let a = i as f32 * 2.399963;
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / 400.0;
            let r = (1.0 - y * y).sqrt();
            Vec3::new(r * a.cos(), y, r * a.sin())
        }).chain([Vec3::Y, -Vec3::Y, Vec3::X]).collect();
        for kind in [0u8, 1, 3] {
            for (i, dir) in dirs.iter().enumerate() {
                for life in [0.05, 0.9, 1.19, 1.5, 4.9] {
                    let speed = match kind { 1 => 420.0, 0 => 60.0, _ => 80.0 };
                    let d = crate::sim::Disc { pos: Vec3::new(100.0, 50.0, 100.0), vel: *dir * speed,
                        team: crate::sim::Team::Glacier, owner: 0, life, kind, spin: i as f32 * 0.7, target: 0, power: 1.0 };
                    let (mut lit, mut emit) = (Vec::new(), Vec::new());
                    match kind {
                        0 => push_disc_round(&mut lit, &mut emit, &d),
                        3 => push_plasma(&mut lit, &mut emit, &d, i as f32 * 0.01),
                        _ => push_tracer(&mut lit, &mut emit, &d, i as f32 * 0.013),
                    }
                    assert!(!lit.is_empty(), "kind {kind} draws a body");
                    for l in &lit {
                        assert!(l.model.is_finite() && normal_columns(l.model).iter().flatten().all(|v| v.is_finite()),
                            "kind {kind}: degenerate transform renders black");
                        assert!(self_lit_luma(l) >= 0.3,
                            "kind {kind}: a part with color {:?} emit {} reads dark", l.color, l.emit);
                    }
                    for e in &emit {
                        // Both signs go through the additive emit pass (negative
                        // alpha only softens the edge), so glow can never darken.
                        assert!(e.color[3] != 0.0 && e.color.iter().all(|c| c.is_finite()),
                            "kind {kind}: projectile glow must be finite");
                        assert!(e.color[0].max(e.color[1]).max(e.color[2]) >= 0.4, "kind {kind}: glow too dark");
                    }
                }
            }
        }
    }

    /// A round nearly at rest (a fizzle, or a spawn frame) still builds a
    /// finite streak instead of a zero-axis matrix.
    #[test]
    fn a_resting_round_keeps_a_finite_transform() {
        for kind in [0u8, 1, 3] {
            let d = crate::sim::Disc { pos: Vec3::new(10.0, 5.0, 10.0), vel: Vec3::ZERO,
                team: crate::sim::Team::Ember, owner: 0, life: 0.5, kind, spin: 0.0, target: 0, power: 1.0 };
            let (mut lit, mut emit) = (Vec::new(), Vec::new());
            match kind {
                0 => push_disc_round(&mut lit, &mut emit, &d),
                3 => push_plasma(&mut lit, &mut emit, &d, 0.0),
                _ => push_tracer(&mut lit, &mut emit, &d, 0.0),
            }
            for l in &lit {
                assert!(normal_columns(l.model).iter().flatten().all(|v| v.is_finite()), "kind {kind}");
            }
        }
    }
}

/// What an armor has fixed to its body (`registry::Mount`): a plate in
/// front, a small gun on the shoulder, a box on the back or belt.
/// `accent`: their livery's trim (`player_model::Livery::accent`).
fn push_mounts(lit: &mut Vec<LitDraw>, p: &crate::sim::Player, accent: Vec3) {
    use peakrunner_core::registry::MountAt;
    let mounts = &p.armor.def().mounts;
    if mounts.is_empty() || !p.alive { return; }
    let turn = Mat4::from_rotation_y(p.yaw);
    let steel = Vec3::new(0.32, 0.35, 0.38);
    for m in mounts.values() {
        // Where the sim has it (`World::mount_point`): a mounted gun's
        // shots leave there.
        let (fwd, right) = (Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos()), Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin()));
        let at = match m.at {
            MountAt::Shoulder => p.pos + Vec3::Y * (peakrunner_core::terrain::EYE - 0.15) + right * 0.45,
            MountAt::Back => p.pos + Vec3::Y * 1.1 - fwd * 0.45,
            MountAt::Front => p.pos + Vec3::Y * 0.9 + fwd * 0.55,
            MountAt::Belt => p.pos + Vec3::Y * 0.6 + right * 0.4,
        };
        if !m.parts.is_empty() {
            let mut glow = Vec::new();
            push_parts(lit, &mut glow, Mat4::from_translation(at) * turn, &m.parts, accent);
            continue;
        }
        if m.blocker.is_some() {
            // A plate square to the front, team-trimmed.
            lit.push(LitDraw { mesh: MeshId::Bevel, model: Mat4::from_translation(at) * turn * Mat4::from_scale(Vec3::new(0.62, 0.8, 0.05)),
                color: steel, emit: 0.0, mode: 0.0 });
            lit.push(LitDraw { mesh: MeshId::Cube, model: Mat4::from_translation(at + Vec3::Y * 0.78 + fwd * 0.03) * turn * Mat4::from_scale(Vec3::new(0.6, 0.04, 0.03)),
                color: accent, emit: 0.6, mode: 0.0 });
        }
        if m.weapon.is_some() {
            // A short barrel on a block, pointing where they look.
            lit.push(LitDraw { mesh: MeshId::Cube, model: Mat4::from_translation(at) * turn * Mat4::from_scale(Vec3::new(0.12, 0.1, 0.16)),
                color: steel, emit: 0.0, mode: 0.0 });
            let aim = Mat4::from_translation(at + Vec3::new(-p.yaw.sin() * p.pitch.cos(), p.pitch.sin(), -p.yaw.cos() * p.pitch.cos()) * 0.25)
                * turn * Mat4::from_rotation_x(p.pitch);
            lit.push(LitDraw { mesh: MeshId::Capsule, model: aim * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2) * Mat4::from_scale(Vec3::new(0.035, 0.3, 0.035)),
                color: steel * 0.6, emit: 0.0, mode: 0.0 });
        }
        if m.blocker.is_none() && m.weapon.is_none() {
            lit.push(LitDraw { mesh: MeshId::Cube, model: Mat4::from_translation(at) * turn * Mat4::from_scale(Vec3::new(0.18, 0.16, 0.12)),
                color: steel, emit: 0.0, mode: 0.0 });
        }
    }
}

/// A map's moving parts and items (`features`): slabs, doors and lifts
/// where they are this step; items while they're there.
fn push_map_features(lit: &mut Vec<LitDraw>, world: &World) {
    use crate::sim::features::{self, MoveableLook};
    let steel = Vec3::new(0.36, 0.39, 0.42);
    for (k, m) in features::moveables(world.map).iter().enumerate() {
        let at = m.at(world.steps, world.moveable_open.get(k).copied().unwrap_or(0.0));
        let frame = Mat4::from_translation(at) * Mat4::from_rotation_y(m.yaw.to_radians());
        let half = Vec3::from(m.size);
        let (body, trim) = match m.look {
            MoveableLook::Platform => (steel, Vec3::new(0.95, 0.75, 0.25)),
            MoveableLook::Door => (Vec3::new(0.3, 0.32, 0.36), if m.team == 1 { team_accent(Team::Glacier) } else { team_accent(Team::Ember) }),
            MoveableLook::Lift => (steel * 0.8, Vec3::new(0.4, 0.9, 0.5)),
        };
        lit.push(LitDraw { mesh: MeshId::Bevel, model: frame * Mat4::from_scale(half * 2.0), color: body, emit: 0.0, mode: 0.0 });
        // A lit band round its top edge.
        lit.push(LitDraw { mesh: MeshId::Cube, model: frame * Mat4::from_translation(Vec3::Y * (half.y - 0.03))
            * Mat4::from_scale(Vec3::new(half.x * 2.02, 0.05, half.z * 2.02)), color: trim, emit: 0.5, mode: 0.0 });
    }
    let items = &peakrunner_core::registry::registry().items;
    for (k, it) in features::map_items(world.map).iter().enumerate() {
        if world.map_item_clocks.get(k).is_some_and(|&c| c > 0.0) { continue; }
        let Some(id) = items.index(&it.item) else { continue };
        let l = crate::sim::loadout::Loot { pos: Vec3::from(it.at), item: id as u8 + 1, ..Default::default() };
        push_loot(lit, &l, None, world.time);
    }
}

/// A vehicle (`vehicles`) by its look: a skiff (a sleek hull with swept
/// wings and a canopy) or a transport (a boxy hull with an open bay), its
/// team's trim, and thrusters that glow while it flies; or its parts. A
/// flier banks as it turns toward where its pilot looks; a ground vehicle
/// tilts with the ground under it. Parts that `aim` turn with their seat's
/// rider's view.
fn push_vehicle(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, c: &crate::sim::vehicles::Craft, world: &crate::sim::World) {
    use peakrunner_core::registry::VehicleLook;
    let time = world.time;
    let Some(def) = c.def() else { return };
    let h = Vec3::from(def.size);
    let right = Vec3::new(c.yaw.cos(), 0.0, -c.yaw.sin());
    let turn = |a: f32| a.sin().atan2(a.cos());
    let view = |k: usize| c.rider(k).and_then(|i| world.players.get(i)).map(|p| (p.yaw, p.pitch));
    let (frame, tilt) = if c.grounded() {
        let ground = |s: f32| terrain::support_on(world.map, c.pos + right * (s * h.x) + Vec3::Y * (0.6 - h.y)).0;
        let resting = c.pos.y - h.y - ground(0.0) < 0.3;
        let roll = if resting { ((ground(1.0) - ground(-1.0)) / (2.0 * h.x)).atan().clamp(-0.6, 0.6) } else { 0.0 };
        (Mat4::from_translation(c.pos) * Mat4::from_rotation_y(c.yaw) * Mat4::from_rotation_x(c.pitch) * Mat4::from_rotation_z(roll), c.pitch)
    } else {
        let bank = view(0).map_or(0.0, |(yaw, _)| (turn(yaw - c.yaw) * 1.2).clamp(-0.6, 0.6));
        (Mat4::from_translation(c.pos) * Mat4::from_rotation_y(c.yaw) * Mat4::from_rotation_x(c.pitch * 0.6) * Mat4::from_rotation_z(bank), c.pitch * 0.6)
    };
    if !def.parts.is_empty() {
        let names: Vec<&str> = std::iter::once("pilot").chain(def.seats.keys().map(String::as_str).filter(|k| *k != "pilot")).collect();
        for part in &def.parts {
            let Some(k) = part.aim.as_deref().and_then(|a| names.iter().position(|n| *n == a)) else {
                push_parts(lit, emit, frame, std::slice::from_ref(part), team_accent(c.team));
                continue;
            };
            // About the upright line through its seat; a barrel also tilts about its pivot.
            let at = def.seat_list().get(k).map_or([0.0; 3], |s| s.at);
            let axis = Vec3::new(at[0], 0.0, -at[2]);
            let (yaw, pitch) = view(k).map_or((0.0, 0.0), |(yaw, pitch)| (turn(yaw - c.yaw), pitch - tilt * turn(yaw - c.yaw).cos()));
            let mut m = frame * Mat4::from_translation(axis) * Mat4::from_rotation_y(yaw) * Mat4::from_translation(-axis);
            if part.elevates {
                let pivot = Vec3::from(part.pivot);
                m = m * Mat4::from_translation(pivot) * Mat4::from_rotation_x(pitch.clamp(-0.35, 0.8)) * Mat4::from_translation(-pivot);
            }
            push_parts(lit, emit, m, std::slice::from_ref(part), team_accent(c.team));
        }
        return;
    }
    let team = team_accent(c.team);
    let hull = Vec3::new(0.42, 0.45, 0.5) * (0.6 + 0.4 * (c.hull / def.hull).clamp(0.0, 1.0));
    let dark = Vec3::new(0.1, 0.11, 0.13);
    let part = |lit: &mut Vec<LitDraw>, mesh: MeshId, at: Vec3, size: Vec3, color: Vec3, emit: f32| {
        lit.push(LitDraw { mesh, model: frame * Mat4::from_translation(at) * Mat4::from_scale(size), color, emit, mode: 0.0 });
    };
    match def.look {
        VehicleLook::Skiff => {
            // Nose toward -Z (the way it faces).
            part(lit, MeshId::Bevel, Vec3::ZERO, Vec3::new(h.x * 0.7, h.y * 1.2, h.z * 2.0), hull, 0.0);
            part(lit, MeshId::Bevel, Vec3::new(0.0, -0.1, 0.4), Vec3::new(h.x * 2.0, h.y * 0.25, h.z * 0.8), hull * 0.9, 0.0);
            part(lit, MeshId::Sphere, Vec3::new(0.0, h.y * 0.6, -h.z * 0.3), Vec3::new(h.x * 0.45, h.y * 0.5, h.z * 0.5), dark, 0.2);
            part(lit, MeshId::Cube, Vec3::new(0.0, h.y * 0.62, h.z * 0.2), Vec3::new(h.x * 0.72, 0.06, h.z * 1.2), team, 0.4);
        }
        VehicleLook::Transport => {
            part(lit, MeshId::Bevel, Vec3::ZERO, Vec3::new(h.x * 2.0, h.y * 1.4, h.z * 2.0), hull, 0.0);
            part(lit, MeshId::Bevel, Vec3::new(0.0, h.y * 0.8, -h.z * 0.75), Vec3::new(h.x * 1.2, h.y * 0.6, h.z * 0.45), dark, 0.15);
            for s in [-1.0f32, 1.0] {
                part(lit, MeshId::Cube, Vec3::new(s * h.x * 1.01, h.y * 0.3, 0.0), Vec3::new(0.04, h.y * 0.25, h.z * 1.8), team, 0.4);
            }
        }
    }
    // Thrusters at the back.
    let glow = 0.6 + 0.4 * (time * 20.0 + c.pos.x).sin().abs();
    for s in [-1.0f32, 1.0] {
        part(lit, MeshId::Capsule, Vec3::new(s * h.x * 0.6, 0.0, h.z * 0.95), Vec3::new(0.18, 0.3, 0.18), dark, 0.0);
        let at = frame.transform_point3(Vec3::new(s * h.x * 0.6, 0.0, h.z * 1.15));
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(0.22 * glow)),
            color: [0.5, 0.8, 1.0, 0.7 * glow] });
    }
}

/// A look built from parts (`registry::Part`) in `frame`: each a primitive
/// shape placed, sized and turned there, in its color or `team`'s.
/// `accent`: the color team parts take (`team_accent`, or a livery's).
pub(crate) fn push_parts(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, frame: Mat4, parts: &[peakrunner_core::registry::Part], accent: Vec3) {
    use peakrunner_core::registry::PartMesh;
    for part in parts {
        let mesh = match part.mesh {
            PartMesh::Cube => MeshId::Cube, PartMesh::Bevel => MeshId::Bevel, PartMesh::Sphere => MeshId::Sphere,
            PartMesh::Disc => MeshId::Disc, PartMesh::Capsule => MeshId::Capsule, PartMesh::Dome => MeshId::Dome,
        };
        let [pitch, yaw, roll] = part.turn.map(f32::to_radians);
        let model = frame * Mat4::from_translation(Vec3::from(part.at))
            * Mat4::from_euler(glam::EulerRot::YXZ, yaw, pitch, roll) * Mat4::from_scale(Vec3::from(part.size));
        let color = if part.team { accent } else { Vec3::from(part.color) };
        lit.push(LitDraw { mesh, model, color, emit: part.glow, mode: 0.0 });
        // A strong glow halos.
        if part.glow > 1.0 {
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: model * Mat4::from_scale(Vec3::splat(1.6)), color: [color.x, color.y, color.z, -0.25] });
        }
    }
}

/// What a body carries besides the model: its armor's parts, its pack's
/// parts on its back, and the glow of its effects.
fn push_body_extras(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, p: &crate::sim::Player, time: f32, accent: Vec3) {
    if !p.alive { return; }
    // Parts in metres about the player's place and facing, as their
    // definitions give them; glows round the body as it's drawn
    // (`player_model::drawn_frame`).
    let place = Mat4::from_translation(p.pos) * Mat4::from_rotation_y(p.yaw);
    let body = crate::player_model::drawn_frame(p, p.pos);
    let armor = p.armor.def();
    if !armor.parts.is_empty() { push_parts(lit, emit, place, &armor.parts, accent); }
    if let Some(parts) = p.pack.filter(|k| k.exists()).map(|k| &k.def().parts).filter(|parts| !parts.is_empty()) {
        push_parts(lit, emit, place * Mat4::from_translation(Vec3::new(0.0, 1.1, 0.42)), parts, accent);
    }
    // The repair tool working on its holder: a green glow about the body
    // and motes rising through it (a stream goes to anything else).
    if p.repair_self {
        let pulse = 0.75 + 0.25 * (time * 7.0).sin().abs();
        // A soft glow hugging the body (negative alpha: bright at its heart,
        // fading to nothing at the rim), not a hard bubble.
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: body * Mat4::from_translation(Vec3::Y * 0.93) * Mat4::from_scale(Vec3::new(0.62, 1.15, 0.62)),
            color: [0.25, 1.0, 0.4, -0.55 * pulse] });
        for k in 0..8 {
            let rise = (time * 0.8 + k as f32 * 0.37).fract();
            let a = k as f32 * std::f32::consts::TAU / 8.0 + time * 1.3;
            let at = body.transform_point3(Vec3::new(a.cos() * 0.42, -0.1 + rise * 2.1, a.sin() * 0.42));
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(0.035)),
                color: [0.35, 1.0, 0.5, 0.9 * (1.0 - rise)] });
        }
    }
    let effects = &peakrunner_core::registry::registry().effects;
    for (k, _) in &p.effects {
        let Some([r, g, b, a]) = effects.get(*k as usize).and_then(|e| e.glow) else { continue };
        let pulse = 0.75 + 0.25 * (time * 5.0 + *k as f32).sin();
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: body * Mat4::from_translation(Vec3::Y * 0.9) * Mat4::from_scale(Vec3::new(0.95, 1.35, 0.95)),
            color: [r, g, b, a * pulse] });
    }
}

/// An armor's size and color on its body's draws (looks only).
/// A round's look at its `size` and in its `color` (`registry::Projectile`):
/// its glow grown or shrunk about it, and recolored.
fn dress_round(lit: &mut [LitDraw], emit: &mut [EmitDraw], d: &crate::sim::Disc, round: &peakrunner_core::registry::Projectile) {
    if round.size != 1.0 {
        let grow = Mat4::from_translation(d.pos) * Mat4::from_scale(Vec3::splat(round.size)) * Mat4::from_translation(-d.pos);
        for l in lit.iter_mut() { l.model = grow * l.model; }
        for e in emit.iter_mut() { e.model = grow * e.model; }
    }
    if let Some(c) = round.color {
        let c = Vec3::from(c);
        for l in lit.iter_mut().filter(|l| l.emit > 0.2) { l.color = c; }
        for e in emit.iter_mut() { e.color = [c.x, c.y, c.z, e.color[3]]; }
    }
}

/// A round's authored model (`registry::Projectile::model`), in place of
/// its look (drawn from `first`; a rocket gets its exhaust instead): along
/// its flight, a disc spinning flat, a thrown one tumbling or at rest;
/// painted its `color` all over when it has one.
fn push_round_model(models: &mut Vec<ModelDraw>, lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, first: (usize, usize),
    d: &crate::sim::Disc, round: &peakrunner_core::registry::Projectile, time: f32, side: Vec3) {
    let Some((id, model)) = round.model.as_deref().and_then(|m| crate::model::get(&format!("projectiles/{m}"))) else { return };
    lit.truncate(first.0);
    emit.truncate(first.1);
    let thrown = round.class == peakrunner_core::registry::ProjectileClass::Thrown;
    let turn = if thrown {
        if crate::sim::throwables::resting(d) { Mat4::from_rotation_y(d.spin * 0.4) }
        else { Mat4::from_rotation_y(d.spin * 0.4) * Mat4::from_rotation_x(d.spin * 0.3) }
    } else if round.model.as_deref() == Some("disc") {
        Mat4::from_rotation_y(d.spin)
    } else {
        let dir = d.vel.normalize_or(Vec3::NEG_Z);
        Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::NEG_Z, dir))
    };
    let start = models.len();
    let paint = round.color.map(Vec3::from);
    crate::model::push(models, id, model, Mat4::from_translation(d.pos) * turn, paint.unwrap_or(side), 1.0, &|_| None);
    if paint.is_some() { for m in &mut models[start..] { m.paint = true; } }
    // A rocket's exhaust: a flickering flame out of its tail.
    if round.accel > 0.0 && d.vel.length_squared() > 1.0 {
        let back = -d.vel.normalize();
        let tail = d.pos + back * model_half_length(model);
        let flick = 1.0 + 0.2 * (time * 47.0 + d.spin).sin();
        let along = glam::Quat::from_rotation_arc(Vec3::Y, back);
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(tail + back * 0.22 * flick)
            * Mat4::from_quat(along) * Mat4::from_scale(Vec3::new(0.09, 0.3 * flick, 0.09)), color: [1.0, 0.55, 0.18, 0.75] });
        emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(tail) * Mat4::from_scale(Vec3::splat(0.07)),
            color: [1.0, 0.92, 0.75, 0.9] });
    }
}

/// Half a model's length along Z (its parts' extent), for a tail's place.
fn model_half_length(model: &crate::model::Model) -> f32 {
    model.vertices.chunks_exact(crate::model::VERTEX_FLOATS).map(|v| v[2].abs()).fold(0.0, f32::max)
}

/// A hook's cable (a round that drags: `pull` or `grapple`) back to the
/// shooter's hands while it flies.
fn push_tether_line(lit: &mut Vec<LitDraw>, world: &World, d: &crate::sim::Disc, round: &peakrunner_core::registry::Projectile) {
    if round.pull == 0.0 && round.grapple == 0.0 { return; }
    let Some(p) = world.players.get(d.owner) else { return };
    let from = crate::player_model::drawn_frame(p, p.pos).transform_point3(Vec3::new(0.25, 1.2, -0.4));
    let span = d.pos - from;
    let length = span.length();
    if length < 0.2 { return; }
    let along = Mat4::from_translation((from + d.pos) * 0.5) * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::Z, span / length));
    lit.push(LitDraw { mesh: MeshId::Cube, model: along * Mat4::from_scale(Vec3::new(0.022, 0.022, length)),
        color: Vec3::new(0.30, 0.32, 0.35), emit: 0.0, mode: 0.0 });
}

/// Turrets on capture points (`control::PointTurret`), live in CTF: their
/// authored model (`models/turrets/<deployable>/`) turned to its aim, else
/// their deployable's look, in the holder's team, gray while nobody holds them.
fn push_point_turrets(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, models: &mut Vec<ModelDraw>, world: &World) {
    if world.mode.rules() != peakrunner_core::modes::Rules::Ctf { return; }
    for p in world.points.iter().filter(|p| p.active) {
        let Some(kind) = p.turret else { continue };
        let team = if p.owner == Some(1) { Team::Glacier } else { Team::Ember };
        if let Some((id, model)) = crate::model::get(&format!("turrets/{}", kind.key())) {
            let yaw = (-p.turret_aim.x).atan2(-p.turret_aim.z);
            let (paint, glow) = match p.owner { Some(_) => (crate::player_model::team_color(team), 1.0), None => (Vec3::splat(0.32), 0.25) };
            crate::model::push(models, id, model, Mat4::from_translation(p.turret_at) * Mat4::from_rotation_y(yaw), paint, glow, &|_| None);
            continue;
        }
        let unit = crate::sim::deploy::Deployable { owner: 0, kind, team, pos: p.turret_at, yaw: 0.0,
            health: kind.def().hull, cooldown: p.turret_cooldown, aim: p.turret_aim, age: 0.0 };
        let (start, glow_start) = (lit.len(), emit.len());
        push_deployable(lit, emit, &unit, world.time);
        if p.owner.is_none() {
            for d in &mut lit[start..] { d.color = Vec3::splat(d.color.dot(Vec3::new(0.3, 0.59, 0.11))); d.emit *= 0.3; }
            emit.truncate(glow_start);
        }
    }
}

/// The map's authored models (`map_pack::ModelPlacement`): each placed root
/// of its `models.glb`, painted in the placement's team color.
fn push_map_models(models: &mut Vec<ModelDraw>, world: &World) {
    let Some(pack) = peakrunner_core::map_pack::on(world.map) else { return };
    let Some((id, model)) = crate::model::of_map(pack) else { return };
    for m in &pack.manifest.models {
        let frame = Mat4::from_translation(Vec3::from_array(m.position)) * Mat4::from_rotation_y(m.yaw.to_radians());
        let paint = match m.team {
            Some(t) => crate::player_model::team_color(if t == 1 { Team::Glacier } else { Team::Ember }),
            None => Vec3::splat(0.32),
        };
        for (k, part) in model.parts.iter().enumerate().filter(|(_, p)| p.group == m.model) {
            models.push(ModelDraw { id, part: k as u16, model: frame * part.node, team: paint, glow: 1.0, paint: false, plates: 0.0, shine: 0.0, skin: RIGID });
        }
    }
}

/// `dress_body` for a body's authored models: the same growth, and the
/// tint on its team paint.
fn dress_models(draws: &mut [ModelDraw], p: &crate::sim::Player) {
    let a = p.armor.def();
    if a.scale != 1.0 {
        let at = Mat4::from_translation(p.pos - Vec3::Y * peakrunner_core::terrain::PLAYER_RADIUS);
        let grow = at * Mat4::from_scale(Vec3::splat(a.scale)) * at.inverse();
        for d in draws.iter_mut() { d.model = grow * d.model; }
    }
    if let Some(t) = a.tint {
        for d in draws.iter_mut() { d.team = d.team.lerp(Vec3::from(t), 0.35); }
    }
}

fn dress_body(draws: &mut [LitDraw], p: &crate::sim::Player) {
    let a = p.armor.def();
    if a.scale != 1.0 {
        let at = Mat4::from_translation(p.pos - Vec3::Y * peakrunner_core::terrain::PLAYER_RADIUS);
        let grow = at * Mat4::from_scale(Vec3::splat(a.scale)) * at.inverse();
        for d in draws.iter_mut() { d.model = grow * d.model; }
    }
    if let Some(t) = a.tint {
        let t = Vec3::from(t);
        for d in draws.iter_mut() { d.color = d.color.lerp(t, 0.35); }
    }
}

#[cfg(test)]
mod muzzle_tests {
    use super::*;

    /// Where an authored weapon model's `muzzle` point sits in first person
    /// (at rest), in the view model's camera space.
    fn model_muzzle(model: &crate::model::Model, view: peakrunner_core::registry::View) -> Vec3 {
        let anchor = crate::sim::vm_anchor(view);
        let (yaw, pitch) = crate::sim::vm_turn(view.kind());
        let grip = Mat4::from_translation(VM_GRIP) * Mat4::from_scale(Vec3::splat(VM_SCALE)) * model.point("grip_r").unwrap().inverse();
        anchor + (Mat4::from_rotation_y(yaw) * Mat4::from_rotation_x(pitch) * grip * model.point("muzzle").unwrap()).transform_point3(Vec3::ZERO)
    }

    /// Every weapon with a model, the base game's and Sparkfall's, says
    /// where its barrel ends (`registry::Weapon::muzzle`), and its rounds,
    /// beams and flash leave there in first person, within a centimetre.
    #[test]
    fn rounds_leave_the_barrel_end_drawn() {
        let r = peakrunner_core::registry::registry();
        let mut checked = 0;
        for (k, id, w) in r.weapons.iter() {
            let Some((_, model)) = crate::player_model::weapon_model(k as u8) else { continue };
            assert!(w.muzzle.is_some(), "{id} has a model but no muzzle");
            let sim = crate::sim::weapon_muzzle(Vec3::ZERO, Vec3::NEG_Z, k as u8, crate::sim::VM_FOV);
            let drawn = model_muzzle(model, w.view);
            assert!(sim.distance(drawn) < 0.01, "{id}: rounds leave {sim:?}, the barrel ends at {drawn:?}");
            checked += 1;
        }
        // Sparkfall's weapons, read from its file (activating a mod would
        // change the whole process's definitions).
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/mods/sparkfall/weapons/sparkfall.json")).unwrap();
        let entries = peakrunner_core::jsonc::parse(&text).unwrap();
        for (id, w) in entries.as_object().unwrap() {
            let Some((_, model)) = crate::model::get(&format!("weapons/{id}")) else { continue };
            let muzzle = w.get("muzzle").and_then(|m| serde_json::from_value::<[f32; 3]>(m.clone()).ok());
            let Some(muzzle) = muzzle else { panic!("Sparkfall's {id} has a model but no muzzle") };
            let drawn = (Mat4::from_translation(VM_GRIP) * Mat4::from_scale(Vec3::splat(VM_SCALE)) * model.point("grip_r").unwrap().inverse()
                * model.point("muzzle").unwrap()).transform_point3(Vec3::ZERO);
            assert!(Vec3::from(muzzle).distance(drawn) < 0.01, "Sparkfall's {id}: muzzle {muzzle:?}, the barrel ends at {drawn:?}");
            checked += 1;
        }
        assert!(checked >= 20, "{checked} weapons checked");
    }
}

#[cfg(test)]
mod side_tests {
    use super::*;
    use peakrunner_core::map_catalog::SupportedMode;

    /// In Rabbit and free-for-all a shot's colors name no side; with teams
    /// they're the shooter's team's.
    #[test]
    fn shots_name_no_side_where_there_are_no_teams() {
        let mut w = World::new();
        w.set_mode(SupportedMode::Ctf);
        assert_eq!(side_accent(&w, Team::Ember), team_accent(Team::Ember));
        assert_eq!(side_paint(&w, Team::Glacier), crate::player_model::team_color(Team::Glacier));
        for mode in [SupportedMode::Rabbit, SupportedMode::Deathmatch] {
            w.set_mode(mode);
            for team in [Team::Ember, Team::Glacier] {
                assert_eq!(side_accent(&w, team), crate::player_model::NEUTRAL_ACCENT, "{mode:?}");
                assert_eq!(side_paint(&w, team), crate::player_model::HUNTER_STEEL, "{mode:?}");
            }
        }
    }
}
