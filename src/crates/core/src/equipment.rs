//! Map-authored equipment definitions and authoritative runtime state.
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use crate::{sim::{Player}, terrain::MapId};

/// Constant-velocity interception, including the barrel's forward spawn offset.
/// Plasma flies straight; gravity/steering changes after firing can evade it.
pub fn intercept_time(relative:Vec3, velocity:Vec3, speed:f32, muzzle_offset:f32, lifetime:f32)->Option<f32> {
    if !relative.is_finite() || !velocity.is_finite() || !speed.is_finite()
        || !muzzle_offset.is_finite() || !lifetime.is_finite() || speed<=0. || muzzle_offset<0. || lifetime<=0. {return None;}
    let r=relative.as_dvec3();let v=velocity.as_dvec3();let s=speed as f64;let o=muzzle_offset as f64;
    let a=v.length_squared()-s*s;
    let b=2.*(r.dot(v)-o*s);
    let c=r.length_squared()-o*o;
    let valid=|t:f64|t.is_finite() && t>0. && t<=lifetime as f64;
    if a.abs()<1e-8 {
        let t=-c/b;return valid(t).then_some(t as f32);
    }
    let discriminant=b*b-4.*a*c;
    if discriminant<0. {return None;}
    let root=discriminant.sqrt();
    [(-b-root)/(2.*a),(-b+root)/(2.*a)].into_iter().filter(|t|valid(*t))
        .min_by(f64::total_cmp).map(|t|t as f32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Kind { Inventory, Generator, Sensor, Turret, Repair }

/// A fixed turret's weapon. `Rocket` fires the stock library's
/// `rocket-turret` (`ROCKET_TURRET`): heat-seeking rockets, only at enemies
/// using their jets, that follow the one they were fired at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum TurretWeapon { #[default] Bullet, Plasma, Rocket }

/// The library deployable whose targeting, fire and look a fixed rocket
/// turret takes.
pub const ROCKET_TURRET: &str = "rocket-turret";

/// Detection and firing numbers for anything that acquires targets. Fixed map
/// turrets and future player-placed turrets look up the same table.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurretProfile {
    /// Detection range without a powered same-team sensor.
    pub range: f32,
    /// Detection range while a same-team sensor is powered.
    pub sensed_range: f32,
    pub speed: f32,
    pub life: f32,
    pub cooldown: f32,
    /// Aim at the constant-velocity intercept point instead of the target.
    pub leads: bool,
    pub fires: bool,
    /// Projectile kind stored in `Disc::kind`.
    pub projectile: u8,
    /// Engages only enemies using their jets (a heat-seeking turret).
    #[serde(default)]
    pub jetting_only: bool,
}


pub fn profile(kind:Kind, weapon:TurretWeapon)->Option<TurretProfile> {
    let e=&crate::defs::defs().equipment;
    match (kind,weapon) {
        (Kind::Sensor,_)=>Some(e.sensor),
        (Kind::Turret,TurretWeapon::Bullet)=>Some(e.bullet_turret),
        (Kind::Turret,TurretWeapon::Plasma)=>Some(e.plasma_turret),
        (Kind::Turret,TurretWeapon::Rocket)=>crate::sim::deploy::DeployKind::from_key(ROCKET_TURRET).and_then(|k|k.turret()),
        _=>None,
    }
}

/// Hull and generator-powered shield numbers per kind. Shields only exist on
/// sensors and fixed turrets: generators are the thing attackers go for, and
/// stations stay as they were. A shield soaks damage before the hull and
/// regenerates continuously, even under fire, while its circuit is powered,
/// faster than one player can deal damage: breaking it takes focus fire or the
/// generator. It drops to zero the moment the circuit loses power.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Durability {
    pub hull: f32,
    pub shield_max: f32,
    /// Shield points per second, applied every powered tick including while hit.
    pub shield_regen_rate: f32,
    /// Fraction of bullet (chaingun and bullet-turret) damage a shield takes;
    /// shields shrug off small arms, explosives hit them in full.
    pub shield_bullet_factor: f32,
}

/// A destroyed object stays offline until repaired past this fraction of hull
/// (`defs().equipment.online_fraction`).
pub fn online_fraction()->f32 {crate::defs::defs().equipment.online_fraction}

pub fn durability(kind:Kind)->Durability {
    let e=&crate::defs::defs().equipment;
    let unshielded=|hull|Durability {hull,shield_max:0.,shield_regen_rate:0.,shield_bullet_factor:1.};
    match kind {
        Kind::Turret=>e.turret_hull,
        Kind::Sensor=>e.sensor_hull,
        Kind::Generator=>unshielded(e.generator_hull),
        Kind::Inventory|Kind::Repair=>unshielded(e.station_hull),
    }
}

/// Barrel clearance beyond the turret's body radius; LOS and shots start here.
pub const MUZZLE_GAP: f32 = 0.6;
/// Targets are sighted at the chest, not the feet.
pub const CHEST_HEIGHT: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate { pub index: usize, pub team: u8, pub pos: Vec3, pub vel: Vec3, pub jetting: bool }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Acquired {
    pub index: usize,
    /// Where the turret points (the intercept point for leading weapons).
    pub aim_point: Vec3,
    pub aim: Vec3,
    pub muzzle: Vec3,
}

/// The one targeting rule: the nearest living enemy within range whose chest
/// is visible from the barrel start. `clear(from,to)` is the caller's line of
/// sight query, so callers can add blockers such as placed objects. Before
/// firing, callers must also check `clear(acquired.muzzle, acquired.aim_point)`.
pub fn acquire_target(origin:Vec3, radius:f32, team:u8, profile:&TurretProfile, sensed:bool,
    candidates:impl IntoIterator<Item=Candidate>, mut clear:impl FnMut(Vec3,Vec3)->bool)->Option<Acquired> {
    let range=if sensed {profile.sensed_range} else {profile.range};
    let (_,index,target,velocity)=candidates.into_iter().filter(|c|c.team!=team && (c.jetting || !profile.jetting_only))
        .filter_map(|c| {
            let target=c.pos+Vec3::Y*CHEST_HEIGHT;let delta=target-origin;
            if delta.length()>range {return None;}
            let start=origin+delta.normalize_or_zero()*(radius+MUZZLE_GAP);
            clear(start,target).then_some((delta.length(),c.index,target,c.vel))
        }).min_by(|a,b|a.0.total_cmp(&b.0))?;
    let aim_point=if profile.leads {
        intercept_time(target-origin,velocity,profile.speed,radius+MUZZLE_GAP,profile.life)
            .map_or(target,|t|target+velocity*t)
    } else {target};
    let aim=(aim_point-origin).normalize_or_zero();
    Some(Acquired {index,aim_point,aim,muzzle:origin+aim*(radius+MUZZLE_GAP)})
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub id: String,
    pub kind: Kind,
    pub position: [f32;3],
    pub team: u8,
    pub circuit: String,
    pub radius: f32,
    #[serde(default)]
    pub weapon: TurretWeapon,
    /// Horizontal (x, z) direction the turret faces. With `arc` below 360 it
    /// only engages targets within `arc / 2` degrees of this direction, which
    /// keeps a turret standing in front of an open doorway from firing back
    /// into its own rooms. Absent means it covers all directions. For an
    /// inventory station: the way its booth opens; players step in from
    /// there (`loadout::station_front`; absent, the engine finds it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<[f32;2]>,
    #[serde(default = "full_arc")]
    pub arc: f32,
}
fn full_arc()->f32 {360.}
/// Horizontal distance within which a turret with a limited field of fire
/// still engages in every direction.
pub const ALL_ROUND_RANGE: f32 = 15.;
impl Definition {
    pub fn pos(&self)->Vec3 {Vec3::from_array(self.position)}
    /// Whether this turret may engage `target`: anywhere within
    /// `ALL_ROUND_RANGE` (it guards its own surroundings, bridge and ramps
    /// included), and beyond that only inside its horizontal field of fire.
    pub fn in_arc(&self, target:Vec3)->bool {
        let Some(f)=self.facing else {return true};
        if self.arc>=360. {return true;}
        let d=Vec2::new(target.x-self.position[0],target.z-self.position[2]);
        let f=Vec2::from_array(f);
        if d.length()<=ALL_ROUND_RANGE || f.length_squared()<1e-6 {return true;}
        d.normalize().dot(f.normalize())>=(self.arc.to_radians()/2.).cos()
    }
    pub fn max_health(&self)->f32 {durability(self.kind).hull}
    pub fn max_shield(&self)->f32 {durability(self.kind).shield_max}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub health: f32,
    pub powered: bool,
    pub cooldown: f32,
    pub aim: Vec3,
    pub contacts: u8,
    pub shield: f32,
    /// Set when the hull reaches zero; cleared once repaired past
    /// `ONLINE_FRACTION`. An offline object has no power and no shield.
    #[serde(default)]
    pub offline: bool,
}

impl State {
    pub fn new(d:&Definition)->Self {
        State {health:d.max_health(),powered:true,cooldown:0.,aim:Vec3::NEG_Z,contacts:0,shield:d.max_shield(),offline:false}
    }
    /// Damage soaks into the shield first, the overflow reaches the hull.
    /// Returns true when this hit destroyed the object.
    pub fn damage(&mut self, d:&Definition, amount:f32, bullet:bool)->bool {
        if !amount.is_finite() || amount<=0. || self.health<=0. {return false;}
        let factor=if bullet {durability(d.kind).shield_bullet_factor} else {1.};
        let mut rest=amount;
        if self.shield>0. && factor>0. {
            let absorbed=(rest*factor).min(self.shield);
            self.shield-=absorbed;rest-=absorbed/factor;
        }
        self.health=(self.health-rest).max(0.);
        if self.health<=0. {self.shield=0.;self.offline=true;return true;}
        false
    }
    /// Advance shield regeneration. Call after `power` for the tick.
    pub fn regen(&mut self, d:&Definition, dt:f32) {
        let spec=durability(d.kind);
        if !self.powered || self.health<=0. {self.shield=0.;return;}
        self.shield=(self.shield+spec.shield_regen_rate*dt).min(spec.shield_max);
    }
    /// Repair adds hull; a destroyed object comes back online only past
    /// `ONLINE_FRACTION` of its hull.
    pub fn repair(&mut self, d:&Definition, amount:f32) {
        self.health=(self.health+amount).min(d.max_health());
        if self.offline && self.health>=d.max_health()*online_fraction() {self.offline=false;}
    }
}

pub fn validate(defs:&[Definition])->Result<(),String> {
    if defs.len()>128 {return Err("Too many equipment objects".into());}
    let mut ids=std::collections::HashSet::new();
    for d in defs {
        if d.id.is_empty() || d.id.len()>64 || !ids.insert(&d.id) || d.team>1
            || d.circuit.is_empty() || d.circuit.len()>64 || !d.radius.is_finite()
            || !(0.5..=8.).contains(&d.radius) || d.position.iter().any(|v|!v.is_finite() || v.abs()>10000.)
            || !d.arc.is_finite() || !(1.0..=360.).contains(&d.arc)
            || d.facing.is_some_and(|f|f.iter().any(|v|!v.is_finite()) || Vec2::from_array(f).length()<1e-3) {
            return Err("Invalid equipment definition".into());
        }
        if d.circuit!="always-on" && !defs.iter().any(|g|g.kind==Kind::Generator && g.team==d.team && g.circuit==d.circuit) {
            return Err(format!("Equipment {} has no same-team generator",d.id));
        }
    }
    Ok(())
}

pub fn definitions(map:MapId)->&'static [Definition] {
    crate::map_pack::on(map).map_or(&[],|p|p.manifest.entities.as_slice())
}
pub fn fresh(map:MapId)->Vec<State> {
    definitions(map).iter().map(State::new).collect()
}
pub fn power(defs:&[Definition],states:&mut [State]) {
    let live: Vec<_>=defs.iter().zip(states.iter()).filter(|(d,s)|d.kind==Kind::Generator && s.health>0. && !s.offline)
        .map(|(d,_)|(d.team,d.circuit.as_str())).collect();
    for (d,s) in defs.iter().zip(states) {
        if s.health<=0. {s.offline=true;}
        s.powered=s.health>0. && !s.offline && (d.circuit=="always-on" || live.contains(&(d.team,d.circuit.as_str())));
        // Shields are projected by the generator: no power, no shield.
        if !s.powered {s.shield=0.;}
    }
}
/// The caller decides who may use the station (its team, or anyone in free-for-all).
pub fn service(d:&Definition,s:&mut State,p:&mut Player,using:bool,dt:f32) {
    if !p.alive || p.pos.distance(d.pos())>d.radius+1.5 {return;}
    // Repairs are the repair tool's job (hold Q, spending energy; see
    // sim::loadout). Stations heal and recharge, and inventories restock ammo.
    if s.powered && s.health>0. && (d.kind==Kind::Repair || (d.kind==Kind::Inventory && using)) {
        p.health=(p.health+35.*dt).min(100.);
        p.energy=(p.energy+40.*dt).min(crate::sim::loadout::energy_max(p.armor));
        if d.kind==Kind::Inventory {p.ammo=crate::sim::loadout::rounds_max(p);crate::sim::World::restock_throwables(p);crate::sim::World::cleanse(p);}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn enemy(index:usize,pos:Vec3)->Candidate {Candidate {index,team:1,pos,vel:Vec3::ZERO,jetting:false}}
    fn open(_:Vec3,_:Vec3)->bool {true}

    #[test]
    fn profile_table_keeps_the_established_numbers() {
        assert_eq!(profile(Kind::Sensor,TurretWeapon::Bullet).map(|p|(p.range,p.sensed_range,p.fires)),Some((260.,260.,false)));
        let bullet=profile(Kind::Turret,TurretWeapon::Bullet).unwrap();
        assert_eq!((bullet.range,bullet.sensed_range,bullet.speed,bullet.life,bullet.cooldown,bullet.leads,bullet.projectile),
            (80.,150.,crate::defs::weapons().chaingun.speed,1.,0.22,false,1));
        let plasma=profile(Kind::Turret,TurretWeapon::Plasma).unwrap();
        assert_eq!((plasma.range,plasma.sensed_range,plasma.speed,plasma.life,plasma.cooldown,plasma.leads,plasma.projectile),
            (80.,150.,80.,3.,1.2,true,3));
        for kind in [Kind::Inventory,Kind::Generator,Kind::Repair] {assert!(profile(kind,TurretWeapon::Bullet).is_none());}
    }

    #[test]
    fn acquire_target_respects_range_sensors_and_teams() {
        let p=crate::defs::defs().equipment.bullet_turret;
        let far=enemy(0,Vec3::new(100.,-0.8,0.));
        assert!(acquire_target(Vec3::ZERO,1.,0,&p,false,[far],open).is_none(),"100 m is outside unsensed range");
        assert_eq!(acquire_target(Vec3::ZERO,1.,0,&p,true,[far],open).map(|a|a.index),Some(0),"a powered sensor extends range");
        let friend=Candidate {index:1,team:0,pos:Vec3::new(5.,-0.8,0.),vel:Vec3::ZERO,jetting:false};
        assert!(acquire_target(Vec3::ZERO,1.,0,&p,false,[friend],open).is_none(),"teammates are never targets");
    }

    #[test]
    fn acquire_target_picks_the_nearest_visible_enemy() {
        let p=crate::defs::defs().equipment.bullet_turret;
        let near=enemy(0,Vec3::new(10.,-0.8,0.));let farther=enemy(1,Vec3::new(30.,-0.8,0.));
        assert_eq!(acquire_target(Vec3::ZERO,1.,0,&p,false,[farther,near],open).map(|a|a.index),Some(0));
        let blocked_near=|_:Vec3,to:Vec3|to.x>20.;
        assert_eq!(acquire_target(Vec3::ZERO,1.,0,&p,false,[farther,near],blocked_near).map(|a|a.index),Some(1),
            "a blocked nearer enemy is skipped");
        assert!(acquire_target(Vec3::ZERO,1.,0,&p,false,[near],|_,_|false).is_none());
    }

    #[test]
    fn acquire_target_sights_from_the_barrel_to_the_chest() {
        let mut seen=Vec::new();
        let hit=acquire_target(Vec3::ZERO,1.5,0,&crate::defs::defs().equipment.bullet_turret,false,[enemy(0,Vec3::new(20.,-0.8,0.))],
            |a,b|{seen.push((a,b));true}).unwrap();
        assert_eq!(seen,vec![(Vec3::new(1.5+MUZZLE_GAP,0.,0.),Vec3::new(20.,0.,0.))]);
        assert_eq!((hit.aim_point,hit.aim,hit.muzzle),(Vec3::new(20.,0.,0.),Vec3::X,Vec3::new(2.1,0.,0.)));
    }

    #[test]
    fn leading_profiles_aim_at_the_intercept_point() {
        let mover=Candidate {index:0,team:1,pos:Vec3::new(40.,-0.8,0.),vel:Vec3::new(0.,0.,20.),jetting:false};
        let plain=acquire_target(Vec3::ZERO,1.,0,&crate::defs::defs().equipment.bullet_turret,false,[mover],open).unwrap();
        let led=acquire_target(Vec3::ZERO,1.,0,&crate::defs::defs().equipment.plasma_turret,false,[mover],open).unwrap();
        assert_eq!(plain.aim_point,Vec3::new(40.,0.,0.));
        let t=intercept_time(Vec3::new(40.,0.,0.),mover.vel,80.,1.+MUZZLE_GAP,3.).unwrap();
        assert_eq!(led.aim_point,Vec3::new(40.,0.,0.)+mover.vel*t);
        assert!(led.aim.z>0.,"plasma leads a target moving across its line");
    }

    #[test]
    fn always_on_circuits_need_no_fabricated_generator() {
        let d=Definition{id:"station".into(),kind:Kind::Inventory,position:[0.;3],team:0,
            circuit:"always-on".into(),radius:1.5,weapon:TurretWeapon::Bullet,facing:None,arc:360.};
        assert!(validate(&[d.clone()]).is_ok());
        let mut states=vec![State{health:100.,powered:false,cooldown:0.,aim:Vec3::ZERO,contacts:0,shield:0.,offline:false}];
        power(&[d.clone()],&mut states);assert!(states[0].powered);
        states[0].health=0.;power(&[d.clone()],&mut states);assert!(!states[0].powered);
        let mut invalid=d;invalid.circuit="missing-generator".into();assert!(validate(&[invalid]).is_err());
    }
    #[test]
    fn plasma_lead_meets_constant_velocity_targets() {
        let r=Vec3::new(90.,5.,0.);
        for v in [Vec3::ZERO,Vec3::new(0.,0.,45.),Vec3::new(-40.,0.,0.),Vec3::new(35.,0.,0.),Vec3::new(0.,30.,10.),Vec3::new(-80.,0.,0.)] {
            let t=intercept_time(r,v,80.,3.,3.).expect("reachable target");
            let target=r+v*t;let aim=target.normalize();
            assert!((aim*(3.+80.*t)-target).length()<0.001,"missed {v:?}");
        }
        assert!(intercept_time(r,Vec3::X*100.,80.,3.,3.).is_none());
        assert!(intercept_time(r,Vec3::ZERO,80.,3.,0.2).is_none());
        assert!(intercept_time(Vec3::splat(f32::NAN),Vec3::ZERO,80.,3.,3.).is_none());
    }
    #[test]
    fn generator_loss_only_disables_its_own_circuit() {
        let make=|id:&str,kind,team|Definition {id:id.into(),kind,team,circuit:format!("base{team}"),position:[0.;3],radius:2.,weapon:TurretWeapon::Bullet,facing:None,arc:360.};
        let defs=vec![make("a",Kind::Generator,0),make("b",Kind::Inventory,0),make("c",Kind::Generator,1)];
        assert!(validate(&defs).is_ok());
        let mut states:Vec<_>=defs.iter().map(|d|State::new(d)).collect();
        states[0].health=0.;power(&defs,&mut states);
        assert!(!states[0].powered && !states[1].powered && states[2].powered);
        states[0].repair(&defs[0],1.);power(&defs,&mut states);
        assert!(!states[1].powered,"a barely patched generator stays offline");
        states[0].repair(&defs[0],defs[0].max_health()*crate::equipment::online_fraction());power(&defs,&mut states);
        assert!(states[1].powered);
    }
    fn def(id:&str,kind:Kind,circuit:&str)->Definition {
        Definition {id:id.into(),kind,team:0,circuit:circuit.into(),position:[0.;3],radius:2.,weapon:TurretWeapon::Bullet,facing:None,arc:360.}
    }
    #[test]
    fn only_sensors_and_turrets_are_shielded() {
        assert!(durability(Kind::Turret).shield_max>0. && durability(Kind::Sensor).shield_max>0.);
        for kind in [Kind::Generator,Kind::Inventory,Kind::Repair] {assert_eq!(durability(kind).shield_max,0.);}
        assert_eq!(def("t",Kind::Turret,"a").max_health(),250.,"hull numbers are unchanged");
        assert_eq!(def("g",Kind::Generator,"a").max_health(),500.);
        assert_eq!(def("s",Kind::Sensor,"a").max_health(),150.);
    }
    #[test]
    fn shield_absorbs_before_hull_and_bullets_are_halved() {
        let d=def("t",Kind::Turret,"a");let mut s=State::new(&d);
        s.damage(&d,100.,false);assert_eq!((s.shield,s.health),(350.,250.));
        s.damage(&d,8.,true);assert_eq!((s.shield,s.health),(346.,250.),"shields take half of bullet damage");
        s.damage(&d,400.,false);assert_eq!((s.shield,s.health),(0.,196.),"overflow reaches the hull");
        s.shield=2.;s.damage(&d,8.,true);assert_eq!((s.shield,s.health),(0.,192.),"bullet overflow is counted at full value");
        s.damage(&d,1000.,false);assert_eq!((s.shield,s.health),(0.,0.));
        let hp=s.health;s.damage(&d,5.,false);assert_eq!(s.health,hp,"destroyed objects take no more damage");
    }
    #[test]
    fn shields_regenerate_continuously_even_while_hit_but_never_unpowered() {
        let d=def("t",Kind::Turret,"a");let spec=durability(d.kind);let mut s=State::new(&d);
        s.damage(&d,300.,false);let hit=s.shield;
        s.regen(&d,0.1);
        assert!((s.shield-(hit+spec.shield_regen_rate*0.1)).abs()<1e-3,"regen starts at once, no delay");
        // A hit every tick does not pause regeneration.
        let before=s.shield;s.damage(&d,5.,false);s.regen(&d,0.1);
        assert!((s.shield-(before-5.+spec.shield_regen_rate*0.1)).abs()<1e-3);
        for _ in 0..200 {s.regen(&d,0.1);}
        assert_eq!(s.shield,spec.shield_max,"regenerates to full while powered");
        s.damage(&d,300.,false);s.powered=false;
        for _ in 0..200 {s.regen(&d,0.1);}
        assert_eq!(s.shield,0.,"an unpowered object has no shield and cannot regen");
    }
    #[test]
    fn shield_regen_beats_one_player_and_loses_to_two() {
        // Best sustained point-blank damage per player weapon against a shield.
        let disc=crate::defs::defs().blasts.disc.max_damage/crate::defs::weapons().disc.reload;
        let grenade=crate::defs::defs().blasts.grenade_structure.max_damage/crate::sim::weapon_reload(2);
        let chaingun=8.*0.5/crate::sim::weapon_reload(1);
        let best=disc.max(grenade).max(chaingun);
        assert!(crate::defs::defs().equipment.turret_hull.shield_regen_rate>best*1.2,"one player ({best:.1}/s) must not out-damage regen with margin");
        assert!(2.*grenade>crate::defs::defs().equipment.turret_hull.shield_regen_rate*1.3 && 2.*disc>crate::defs::defs().equipment.turret_hull.shield_regen_rate*1.2,"a focused pair breaks it");
    }
    #[test]
    fn generator_loss_drops_shields_and_leaves_the_hull_exposed() {
        let defs=vec![def("g",Kind::Generator,"a"),def("t",Kind::Turret,"a"),def("s",Kind::Sensor,"a")];
        let mut states:Vec<_>=defs.iter().map(State::new).collect();
        assert!(states[0].damage(&defs[0],1000.,false),"the killing blow reports destruction");
        assert!(!states[0].damage(&defs[0],10.,false),"a wreck is not destroyed again");
        power(&defs,&mut states);
        assert!(!states[1].powered && !states[2].powered);
        assert_eq!((states[1].shield,states[2].shield),(0.,0.));
        states[1].damage(&defs[1],100.,false);assert_eq!(states[1].health,150.,"damage goes straight to the hull");
        // A destroyed generator stays offline until repaired past half its hull.
        states[0].repair(&defs[0],defs[0].max_health()*crate::equipment::online_fraction()-1.);power(&defs,&mut states);
        assert!(states[0].offline && !states[1].powered,"still offline below the threshold");
        states[0].repair(&defs[0],1.);power(&defs,&mut states);
        assert!(!states[0].offline && states[1].powered,"back online at the threshold");
        for _ in 0..200 {for (d,s) in defs.iter().zip(&mut states) {s.regen(d,0.1);}}
        assert_eq!(states[1].shield,durability(Kind::Turret).shield_max);
    }
    #[test]
    fn destroyed_turrets_also_need_half_hull_to_work_again() {
        let defs=vec![def("g",Kind::Generator,"a"),def("t",Kind::Turret,"a")];
        let mut states:Vec<_>=defs.iter().map(State::new).collect();
        states[1].damage(&defs[1],10000.,false);power(&defs,&mut states);
        states[1].repair(&defs[1],50.);power(&defs,&mut states);
        assert!(!states[1].powered && states[1].health==50.);
        states[1].repair(&defs[1],80.);power(&defs,&mut states);
        assert!(states[1].powered);
    }
    #[test]
    fn always_on_equipment_keeps_a_regenerating_shield() {
        let d=def("t",Kind::Turret,"always-on");assert!(validate(&[d.clone()]).is_ok());
        let mut states=vec![State::new(&d)];
        states[0].damage(&d,300.,false);power(std::slice::from_ref(&d),&mut states);
        for _ in 0..200 {states[0].regen(&d,0.1);}
        assert_eq!(states[0].shield,durability(Kind::Turret).shield_max);
    }
    #[test]
    fn rejects_missing_generator_and_duplicate_ids() {
        let d=Definition {id:"station".into(),kind:Kind::Inventory,team:0,circuit:"a".into(),position:[0.;3],radius:2.,weapon:TurretWeapon::Bullet,facing:None,arc:360.};
        assert!(validate(&[d.clone()]).is_err());
        let mut g=d.clone();g.kind=Kind::Generator;
        assert!(validate(&[g,d]).is_err());
    }
    #[test]
    fn a_facing_arc_limits_what_a_turret_may_engage() {
        let mut d=def("t",Kind::Turret,"always-on");
        assert!(d.in_arc(Vec3::new(0.,0.,-50.)),"no facing covers every direction");
        d.facing=Some([0.,-1.]);d.arc=200.;
        assert!(validate(&[d.clone()]).is_ok());
        assert!(d.in_arc(Vec3::new(0.,0.,-50.)));
        assert!(d.in_arc(Vec3::new(50.,0.,0.)),"flanks are inside a 200 degree arc");
        assert!(!d.in_arc(Vec3::new(0.,0.,50.)),"straight behind is outside");
        assert!(!d.in_arc(Vec3::new(20.,0.,50.)));
        assert!(d.in_arc(Vec3::new(0.,30.,0.)),"straight above counts as inside");
        assert!(d.in_arc(Vec3::new(0.,0.,ALL_ROUND_RANGE-1.)),"close behind is still covered");
        assert!(!d.in_arc(Vec3::new(0.,0.,ALL_ROUND_RANGE+1.)));
        for bad in [0.,400.,f32::NAN] {let mut b=d.clone();b.arc=bad;assert!(validate(&[b]).is_err());}
        let mut b=d.clone();b.facing=Some([0.,0.]);assert!(validate(&[b]).is_err());
    }
}
