//! Logique pure (sans interface), identique à la version web.
//!
//! Même couleur de case = distance paire ; couleurs différentes = distance impaire.
//! - Glyphe impair (Terre/Eau) : glyphes à distance impaire du Disciple (couleur opposée à lui),
//!   case sûre = même couleur que le Disciple.
//! - Glyphe pair (Air/Feu) : glyphes à distance paire (même couleur que lui),
//!   case sûre = couleur opposée.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Light,
    Dark,
}

impl Color {
    pub fn opp(self) -> Self {
        match self {
            Color::Light => Color::Dark,
            Color::Dark => Color::Light,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Color::Light => "claire",
            Color::Dark => "foncée",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Impair, // Terre / Eau
    Pair,   // Air / Feu
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pref {
    Auto,
    Fixed(Kind),
}

#[derive(Clone, Copy, Debug)]
pub struct State {
    pub me: Option<Color>,
    pub dis: Option<Color>,
    pub kim: Option<Color>,
    pub pref: Pref,
    pub statue: bool,
}

impl Default for State {
    fn default() -> Self {
        Self { me: None, dis: None, kim: None, pref: Pref::Auto, statue: false }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub kind: Kind,
    /// Couleur des cases qui seront glyphées (mortelles en début de tour).
    pub glyph: Color,
    /// Couleur de case sûre pour mon personnage.
    pub safe: Color,
    pub kim_ok: bool,
    /// Statue + duo imposé : il faut d'abord pousser le Disciple d'un nombre impair de cases.
    pub push_disciple_first: bool,
    /// Hors Statue : case sûre si on pousse le Disciple d'un nombre impair de cases.
    pub alt_safe_if_disciple_pushed: Option<Color>,
    pub me_placed: bool,
}

fn plan_for(kind: Kind, d: Color) -> (Color, Color) {
    match kind {
        Kind::Impair => (d.opp(), d), // (glyph, safe)
        Kind::Pair => (d, d.opp()),
    }
}

pub fn compute(s: &State) -> Option<Plan> {
    let (me, dis, kim) = (s.me?, s.dis?, s.kim?);
    let mut d = dis;
    let mut push_disciple_first = false;

    let kind = if s.statue {
        // Statue : mon personnage ne bouge pas, donc ma case doit être sûre.
        let mut k = if me == dis { Kind::Impair } else { Kind::Pair };
        if let Pref::Fixed(f) = s.pref {
            if f != k {
                k = f;
                d = dis.opp();
                push_disciple_first = true;
            }
        }
        k
    } else {
        match s.pref {
            Pref::Fixed(f) => f,
            Pref::Auto => {
                if kim != dis {
                    Kind::Impair
                } else {
                    Kind::Pair
                }
            }
        }
    };

    let (glyph, safe) = plan_for(kind, d);
    let alt = if s.statue { None } else { Some(plan_for(kind, dis.opp()).1) };

    Some(Plan {
        kind,
        glyph,
        safe,
        kim_ok: kim == glyph,
        push_disciple_first,
        alt_safe_if_disciple_pushed: alt,
        me_placed: me == safe,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Color::*;

    fn st(me: Color, dis: Color, kim: Color, statue: bool, pref: Pref) -> State {
        State { me: Some(me), dis: Some(dis), kim: Some(kim), pref, statue }
    }

    #[test]
    fn auto_sans_statue_le_kimbo_est_toujours_sur_un_glyphe() {
        for me in [Light, Dark] {
            for dis in [Light, Dark] {
                for kim in [Light, Dark] {
                    let p = compute(&st(me, dis, kim, false, Pref::Auto)).unwrap();
                    assert!(p.kim_ok);
                }
            }
        }
    }

    #[test]
    fn statue_auto_je_reste_sur_ma_case() {
        for me in [Light, Dark] {
            for dis in [Light, Dark] {
                for kim in [Light, Dark] {
                    let p = compute(&st(me, dis, kim, true, Pref::Auto)).unwrap();
                    assert_eq!(p.safe, me);
                    assert!(p.me_placed);
                    assert_eq!(p.kim_ok, kim == me.opp());
                }
            }
        }
    }

    #[test]
    fn statue_duo_impose_pousser_disciple_garde_ma_case_sure() {
        for me in [Light, Dark] {
            for dis in [Light, Dark] {
                for k in [Kind::Impair, Kind::Pair] {
                    let p = compute(&st(me, dis, Light, true, Pref::Fixed(k))).unwrap();
                    assert_eq!(p.kind, k);
                    assert_eq!(p.safe, me);
                }
            }
        }
    }

    #[test]
    fn exemple_impair() {
        // Disciple claire, Kimbo foncée => Impair ; case sûre = claire ; glyphes sur foncée.
        let p = compute(&st(Light, Light, Dark, false, Pref::Auto)).unwrap();
        assert_eq!(p.kind, Kind::Impair);
        assert_eq!(p.safe, Light);
        assert_eq!(p.glyph, Dark);
    }
}

// ---------------------------------------------------------------------------
// Tengu Givrefoux : suivi du combat
// ---------------------------------------------------------------------------

/// Nombre de tours pendant lesquels le Tengu perd son invulnérabilité.
pub const VULN_TURNS: u8 = 4;

#[derive(Clone, Debug, Default)]
pub struct Tengu {
    /// Les monstres qui accompagnent le Tengu sont éliminés (Yokaï et Yomi encore en vie).
    pub cleared: bool,
    /// Yokaï poussé/attiré dans la diagonale du Tengu.
    pub yokai: bool,
    /// Yomi aligné avec le Tengu et frappé avec une arme.
    pub yomi: bool,
    /// Tours restants où le Tengu est vulnérable (None = pas encore retiré).
    pub vuln: Option<u8>,
}

impl Tengu {
    pub fn ready(&self) -> bool {
        self.cleared && self.yokai && self.yomi
    }
    pub fn start_vulnerable(&mut self) {
        if self.ready() && self.vuln.is_none() {
            self.vuln = Some(VULN_TURNS);
        }
    }
    pub fn next_turn(&mut self) {
        if let Some(v) = self.vuln {
            self.vuln = Some(v.saturating_sub(1));
        }
    }
    /// Le Tengu est redevenu invulnérable : on refait Yokaï + Yomi (les monstres restent éliminés).
    pub fn restart_cycle(&mut self) {
        self.yokai = false;
        self.yomi = false;
        self.vuln = None;
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tengu_tests {
    use super::*;

    #[test]
    fn cycle_complet() {
        let mut t = Tengu::default();
        t.start_vulnerable();
        assert_eq!(t.vuln, None); // pas prêt
        t.cleared = true;
        t.yokai = true;
        t.yomi = true;
        t.start_vulnerable();
        assert_eq!(t.vuln, Some(4));
        for _ in 0..6 {
            t.next_turn();
        }
        assert_eq!(t.vuln, Some(0));
        t.restart_cycle();
        assert!(t.cleared && !t.yokai && !t.yomi && t.vuln.is_none());
    }
}
