mod fixed;
mod variable;
mod variable_linear;

pub use fixed::*;
pub use variable::*;
pub use variable_linear::*;

pub type CurveCursor = u16;

pub trait Curve {
    type Output;

    fn duration(&self) -> f32;

    /// Easier to use sampling method that doesn't needs the keyframe cursor,
    /// but is more expensive in some types of curve, been always `O(n)`.
    ///
    /// This means sampling is more expensive to evaluate as the `time` gets bigger;
    fn sample(&self, time: f32) -> Self::Output;

    /// Samples the curve starting from some keyframe cursor, this make the common case `O(1)`
    ///
    /// **NOTE** Each keyframe is indexed by a `u16` to reduce memory usage when using the keyframe caching
    fn sample_with_cursor(&self, cursor: u16, time: f32) -> (CurveCursor, Self::Output);
}
