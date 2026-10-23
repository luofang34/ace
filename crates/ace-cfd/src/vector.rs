pub(crate) type Vector = [f64; 3];

pub(crate) fn dot(a: Vector, b: Vector) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}

pub(crate) fn add(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|i| a[i] + b[i])
}

pub(crate) fn sub(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|i| a[i] - b[i])
}

pub(crate) fn scale(a: Vector, k: f64) -> Vector {
    a.map(|v| v * k)
}

pub(crate) fn norm(a: Vector) -> f64 {
    dot(a, a).sqrt()
}
