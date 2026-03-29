use crate::{
    any::difficulty::object::{HasStartTime, IDifficultyObject},
    mania::difficulty::object::ManiaDifficultyObject,
    util::difficulty::logistic,
};

define_skill! {
    #[allow(clippy::struct_field_names)]
    pub struct Strain: StrainDecaySkill => [ManiaDifficultyObject][ManiaDifficultyObject] {
        start_times: Box<[f64]>,
        end_times: Box<[f64]>,
        individual_strains: Box<[f64]>,

        individual_strain: f64 = 0.0,
        overall_strain: f64 = 1.0,
    }

    pub fn new(total_columns: usize) -> Self {
        Self {
            start_times: vec![0.0; total_columns].into_boxed_slice(),
            end_times: vec![0.0; total_columns].into_boxed_slice(),
            individual_strains: vec![0.0; total_columns].into_boxed_slice(),
            individual_strain: 0.0,
            overall_strain: 1.0,
        }
    }
}

impl Strain {
    const INDIVIDUAL_DECAY_BASE: f64 = 0.125;
    const OVERALL_DECAY_BASE: f64 = 0.3;
    const RELEASE_THRESHOLD: f64 = 30.0;

    const SKILL_MULTIPLIER: f64 = 1.0;
    const STRAIN_DECAY_BASE: f64 = 1.0;

    fn calculate_initial_strain(
        &self,
        offset: f64,
        curr: &ManiaDifficultyObject,
        objects: &[ManiaDifficultyObject],
    ) -> f64 {
        let prev_start_time = curr
            .previous(0, objects)
            .map_or(0.0, HasStartTime::start_time);

        apply_decay(
            self.individual_strain,
            offset - prev_start_time,
            Self::INDIVIDUAL_DECAY_BASE,
        ) + apply_decay(
            self.overall_strain,
            offset - prev_start_time,
            Self::OVERALL_DECAY_BASE,
        )
    }

    fn strain_value_of(
        &mut self,
        curr: &ManiaDifficultyObject,
        objects: &[ManiaDifficultyObject],
    ) -> f64 {
        let start_time = curr.start_time;
        let end_time = curr.end_time;
        let column = curr.base_column;

        let mut is_overlapping = false;
        let mut closest_end_time = (end_time - start_time).abs();

        let mut hold_factor = 1.0;
        let mut hold_addition = 0.0;

        // ========================
        // LN detection (simple)
        // ========================
        for i in 0..self.end_times.len() {
            is_overlapping |= self.end_times[i] > start_time + 1.0
                && end_time > self.end_times[i] + 1.0
                && start_time > self.start_times[i] + 1.0;

            if self.end_times[i] > end_time + 1.0 && start_time > self.start_times[i] + 1.0 {
                hold_factor = 1.25;
            }

            closest_end_time = (end_time - self.end_times[i]).abs().min(closest_end_time);
        }

        if is_overlapping {
            hold_addition =
                logistic(closest_end_time, Self::RELEASE_THRESHOLD, 0.27, None);
        }

        // ========================
        // Individual strain
        // ========================
        self.individual_strains[column] = apply_decay(
            self.individual_strains[column],
            start_time - self.start_times[column],
            Self::INDIVIDUAL_DECAY_BASE,
        );

        self.individual_strains[column] += 2.0 * hold_factor;

        self.individual_strain = if curr.delta_time <= 1.0 {
            self.individual_strain.max(self.individual_strains[column])
        } else {
            self.individual_strains[column]
        };

        // ========================
        // Overall strain
        // ========================
        self.overall_strain = apply_decay(
            self.overall_strain,
            curr.delta_time,
            Self::OVERALL_DECAY_BASE,
        );

        self.overall_strain += (1.0 + hold_addition) * hold_factor;

        // ========================
        // Update state
        // ========================
        self.start_times[column] = start_time;
        self.end_times[column] = end_time;

        self.individual_strain
            + self.overall_strain
            - self.strain_decay_skill_current_strain
    }
}

// ========================
// Decay function
// ========================
fn apply_decay(value: f64, delta_time: f64, decay_base: f64) -> f64 {
    value * f64::powf(decay_base, delta_time / 1000.0)
            }
