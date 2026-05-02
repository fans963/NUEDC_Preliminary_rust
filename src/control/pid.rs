use core::ops::{Add, Div, Mul, Sub};

pub trait PidScalar: Copy + PartialOrd + Default {}

impl PidScalar for f32 {}
impl PidScalar for f64 {}

pub trait IntegralLimit<T>: Copy {
    fn clamp(&self, value: T) -> T;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoIntegralLimit;

impl<T> IntegralLimit<T> for NoIntegralLimit {
    fn clamp(&self, value: T) -> T {
        value
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IntegralBounds<T> {
    min: T,
    max: T,
}

impl<T> IntegralBounds<T> {
    pub fn new(min: T, max: T) -> Self {
        Self { min, max }
    }
}

impl<T> IntegralLimit<T> for IntegralBounds<T>
where
    T: Copy + PartialOrd,
{
    fn clamp(&self, value: T) -> T {
        if value < self.min {
            self.min
        } else if value > self.max {
            self.max
        } else {
            value
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Pid<T, S = T, L = NoIntegralLimit> {
    kp: S,
    ki: S,
    kd: S,
    integral: T,
    previous_error: T,
    last_output: T,
    integral_limit: L,
}

pub type ScalarPid<T> = Pid<T, T, NoIntegralLimit>;

impl<T, S, L> Pid<T, S, L>
where
    T: Copy + Default + Add<Output = T> + Sub<Output = T> + Mul<S, Output = T> + Div<S, Output = T>,
    S: PidScalar + Mul<T, Output = T>,
    L: IntegralLimit<T>,
{
    pub fn new(kp: S, ki: S, kd: S) -> Self
    where
        L: Default,
    {
        Self {
            kp,
            ki,
            kd,
            integral: T::default(),
            previous_error: T::default(),
            last_output: T::default(),
            integral_limit: L::default(),
        }
    }

    pub fn reset(&mut self) {
        self.integral = T::default();
        self.previous_error = T::default();
        self.last_output = T::default();
    }

    pub fn update(&mut self, setpoint: T, measurement: T, dt: S) -> T {
        if dt <= S::default() {
            return self.last_output;
        }

        let error = setpoint - measurement;

        self.integral = self.integral + error * dt;
        self.integral = self.integral_limit.clamp(self.integral);

        let derivative = (error - self.previous_error) / dt;
        let output = error * self.kp + self.integral * self.ki + derivative * self.kd;

        self.previous_error = error;
        self.last_output = output;
        output
    }
}

impl<T, S, L> Default for Pid<T, S, L>
where
    T: Copy + Default + Add<Output = T> + Sub<Output = T> + Mul<S, Output = T> + Div<S, Output = T>,
    S: PidScalar + Mul<T, Output = T>,
    L: IntegralLimit<T> + Default,
{
    fn default() -> Self {
        Self::new(S::default(), S::default(), S::default())
    }
}

impl<T, S> Pid<T, S, NoIntegralLimit>
where
    T: Copy + Default + Add<Output = T> + Sub<Output = T> + Mul<S, Output = T> + Div<S, Output = T>,
    S: PidScalar + Mul<T, Output = T>,
{
    pub fn with_integral_limit_strategy<L2>(self, integral_limit: L2) -> Pid<T, S, L2>
    where
        L2: IntegralLimit<T>,
    {
        Pid {
            kp: self.kp,
            ki: self.ki,
            kd: self.kd,
            integral: self.integral,
            previous_error: self.previous_error,
            last_output: self.last_output,
            integral_limit,
        }
    }

    pub fn with_integral_limits(self, min: T, max: T) -> Pid<T, S, IntegralBounds<T>>
    where
        T: PartialOrd,
    {
        self.with_integral_limit_strategy(IntegralBounds::new(min, max))
    }
}
