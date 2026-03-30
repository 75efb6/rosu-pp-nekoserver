use std::f64::consts::PI;

use crate::{
    osu::{
        difficulty::skills::{
            aim::Aim, flashlight::Flashlight, speed::Speed, strain::OsuStrainSkill,
        },
        OsuDifficultyAttributes, OsuPerformanceAttributes, OsuScoreState,
    },
    util::{
        difficulty::reverse_lerp,
        float_ext::FloatExt,
        special_functions::{erf, erf_inv},
    },
    GameMods,
};

use super::{n_large_tick_miss, n_slider_ends_dropped, total_imperfect_hits};

pub const PERFORMANCE_BASE_MULTIPLIER: f64 = 1.15;

pub(super) struct OsuPerformanceCalculator<'mods> {
    attrs: OsuDifficultyAttributes,
    mods: &'mods GameMods,
    acc: f64,
    state: OsuScoreState,
    effective_miss_count: f64,
    using_classic_slider_acc: bool,
}

impl<'a> OsuPerformanceCalculator<'a> {
    pub const fn new(
        attrs: OsuDifficultyAttributes,
        mods: &'a GameMods,
        acc: f64,
        state: OsuScoreState,
        effective_miss_count: f64,
        using_classic_slider_acc: bool,
    ) -> Self {
        Self {
            attrs,
            mods,
            acc,
            state,
            effective_miss_count,
            using_classic_slider_acc,
        }
    }
}

impl OsuPerformanceCalculator<'_> {
    pub fn calculate(mut self) -> OsuPerformanceAttributes {
        let total_hits = self.state.total_hits();

        if total_hits == 0 {
            return OsuPerformanceAttributes {
                difficulty: self.attrs,
                ..Default::default()
            };
        }

        let total_hits = f64::from(total_hits);

        let mut multiplier = PERFORMANCE_BASE_MULTIPLIER;

        if self.mods.rx() {
            let od = self.attrs.od();

            let n50_mult = if od > 0.0 {
                (1.0 - (od / 13.33).powf(5.0)).max(0.0)
            } else {
                1.0
            };

            self.effective_miss_count = (self.effective_miss_count
                + f64::from(self.state.n50) * n50_mult)
                .min(total_hits);
        }

        if self.mods.nf() {
            multiplier *= (1.0 - 0.02 * self.effective_miss_count).max(0.9);
        }

        if self.mods.so() && total_hits > 0.0 {
            multiplier *= 1.0 - (f64::from(self.attrs.n_spinners) / total_hits).powf(0.85);
        }

        let speed_deviation = self.calculate_speed_deviation();

        let mut aim_value = self.compute_aim_value();
        let mut speed_value = self.compute_speed_value(speed_deviation);
        let acc_value = self.compute_accuracy_value();
        let flashlight_value = self.compute_flashlight_value(); 
    
        if self.mods.rx() {
            let aim_strain = self.attrs.aim;
            let speed_strain = self.attrs.speed.max(1e-6);

            let streams_nerf =
                ((aim_strain / speed_strain) * 100.0).round() / 100.0;

            if streams_nerf < 1.09 {
                let acc_factor = (1.0 - self.acc).abs();
                let acc_depression = (0.86 - acc_factor).max(0.5);

                aim_value *= acc_depression;

                // slight speed influence (kept mild)
                speed_value *= acc_depression;
            }
        }

        let pp = (aim_value.powf(1.1)
            + speed_value.powf(1.1)
            + acc_value.powf(1.1)
            + flashlight_value.powf(1.1))
        .powf(1.0 / 1.1)
            * multiplier;

        OsuPerformanceAttributes {
            difficulty: self.attrs,
            pp_acc: acc_value,
            pp_aim: aim_value,
            pp_flashlight: flashlight_value,
            pp_speed: speed_value,
            pp,
            effective_miss_count: self.effective_miss_count,
            speed_deviation,
        }
    }

    fn compute_aim_value(&self) -> f64 {
        if self.mods.ap() {
            return 0.0;
        }

        let mut aim_difficulty = self.attrs.aim;

        if self.attrs.n_sliders > 0 && self.attrs.aim_difficult_slider_count > 0.0 {
            let estimate_improperly_followed_difficult_sliders = if self.using_classic_slider_acc {
                let maximum_possible_dropped_sliders = total_imperfect_hits(&self.state);

                f64::clamp(
                    f64::min(
                        maximum_possible_dropped_sliders,
                        f64::from(self.attrs.max_combo - self.state.max_combo),
                    ),
                    0.0,
                    self.attrs.aim_difficult_slider_count,
                )
            } else {
                f64::clamp(
                    f64::from(
                        n_slider_ends_dropped(&self.attrs, &self.state)
                            + n_large_tick_miss(&self.attrs, &self.state),
                    ),
                    0.0,
                    self.attrs.aim_difficult_slider_count,
                )
            };

            let slider_nerf_factor = (1.0 - self.attrs.slider_factor)
                * f64::powf(
                    1.0 - estimate_improperly_followed_difficult_sliders
                        / self.attrs.aim_difficult_slider_count,
                    3.0,
                )
                + self.attrs.slider_factor;

            aim_difficulty *= slider_nerf_factor;
        }

        let mut aim_value = Aim::difficulty_to_performance(aim_difficulty);

        let total_hits = self.total_hits();

        let len_bonus = 0.95
            + 0.4 * (total_hits / 2000.0).min(1.0)
            + f64::from(u8::from(total_hits > 2000.0)) * (total_hits / 2000.0).log10() * 0.5;

        aim_value *= len_bonus;

        if self.effective_miss_count > 0.0 {
            aim_value *= Self::calculate_miss_penalty(
                self.effective_miss_count,
                self.attrs.aim_difficult_strain_count,
            );
        }

        let ar_factor = if self.attrs.ar > 10.33 {
            0.3 * (self.attrs.ar - 10.33)
        } else if self.attrs.ar < 8.0 {
            0.05 * (8.0 - self.attrs.ar)
        } else {
            0.0
        };

        aim_value *= 1.0 + ar_factor * len_bonus;

        if self.mods.bl() {
            aim_value *= 1.3
                + (total_hits
                    * (0.0016 / (1.0 + 2.0 * self.effective_miss_count))
                    * self.acc.powf(16.0))
                    * (1.0 - 0.003 * self.attrs.hp * self.attrs.hp);
        } else if self.mods.hd() || self.mods.tc() {
            aim_value *= 1.0 + 0.04 * (12.0 - self.attrs.ar);
        }

        aim_value *= self.acc;
        aim_value *= 0.98 + f64::powf(f64::max(0.0, self.attrs.od()), 2.0) / 2500.0;

        aim_value
    }

    // rest of file unchanged...
}
