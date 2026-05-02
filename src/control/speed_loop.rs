use crate::control::pid::{IntegralBounds, Pid};
use crate::sensor::encoder::{As5047p, As5047pError, As5047pVelocityEstimator};
use embedded_hal::spi::SpiDevice;
use uom::si::angular_velocity::radian_per_second;
use uom::si::f32::{AngularVelocity, Time};
use uom::si::time::second;

/// Unit-safe velocity loop wrapper around encoder + scalar PID.
pub struct UomSpeedLoop {
    estimator: As5047pVelocityEstimator,
    pid: Pid<f32, f32, IntegralBounds<f32>>,
}

impl UomSpeedLoop {
    pub fn new(kp: f32, ki: f32, kd: f32, integral_min: f32, integral_max: f32) -> Self {
        Self {
            estimator: As5047pVelocityEstimator::new(),
            pid: Pid::new(kp, ki, kd).with_integral_limits(integral_min, integral_max),
        }
    }

    pub fn reset(&mut self) {
        self.estimator.reset();
        self.pid.reset();
    }

    /// Update loop using an already-read raw angle sample.
    pub fn update_from_raw(
        &mut self,
        setpoint: AngularVelocity,
        raw_angle: u16,
        dt: Time,
    ) -> f32 {
        let measured = self.estimator.update(raw_angle, dt);
        self.pid.update(
            setpoint.get::<radian_per_second>(),
            measured.get::<radian_per_second>(),
            dt.get::<second>(),
        )
    }

    /// Update loop by directly reading AS5047P.
    pub fn update_from_encoder<SPI, SpiE>(
        &mut self,
        encoder: &mut As5047p<SPI>,
        setpoint: AngularVelocity,
        dt: Time,
    ) -> Result<f32, As5047pError<SpiE>>
    where
        SPI: SpiDevice<u8, Error = SpiE>,
    {
        let raw = encoder.read_angle_raw()?;
        Ok(self.update_from_raw(setpoint, raw, dt))
    }
}
