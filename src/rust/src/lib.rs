use napi_derive::napi;

#[napi]
pub struct StellarSandbox {
    _counter: u64,
}

#[napi]
impl StellarSandbox {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self { _counter: 0 }
    }
}
