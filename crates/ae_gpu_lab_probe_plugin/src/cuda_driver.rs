use libloading::{Library, Symbol};
use std::ffi::{c_char, c_uint, c_void, CString};
use std::sync::OnceLock;

type CUcontext = *mut c_void;
type CUstream = *mut c_void;
type CUmodule = *mut c_void;
type CUfunction = *mut c_void;
type CUresult = i32;

type CuInit = unsafe extern "C" fn(c_uint) -> CUresult;
type CuCtxSetCurrent = unsafe extern "C" fn(CUcontext) -> CUresult;
type CuModuleLoadData = unsafe extern "C" fn(*mut CUmodule, *const c_void) -> CUresult;
type CuModuleGetFunction =
    unsafe extern "C" fn(*mut CUfunction, CUmodule, *const c_char) -> CUresult;
type CuLaunchKernel = unsafe extern "C" fn(
    CUfunction,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    CUstream,
    *mut *mut c_void,
    *mut *mut c_void,
) -> CUresult;
type CuStreamSynchronize = unsafe extern "C" fn(CUstream) -> CUresult;

pub fn process_bgra128_pitched(
    context: *mut c_void,
    stream: *mut c_void,
    input: *mut c_void,
    output: *mut c_void,
    width: u32,
    height: u32,
    input_rowbytes: u32,
    output_rowbytes: u32,
    mode: u32,
) -> Result<(), String> {
    let driver = driver()?;
    unsafe {
        check((driver.cu_init)(0), "cuInit")?;
        check((driver.cu_ctx_set_current)(context), "cuCtxSetCurrent")?;

        let mut input_param = input as u64;
        let mut output_param = output as u64;
        let mut width_param = width;
        let mut height_param = height;
        let mut input_rowbytes_param = input_rowbytes;
        let mut output_rowbytes_param = output_rowbytes;
        let mut mode_param = mode;
        let mut params = [
            (&mut input_param as *mut u64).cast::<c_void>(),
            (&mut output_param as *mut u64).cast::<c_void>(),
            (&mut width_param as *mut u32).cast::<c_void>(),
            (&mut height_param as *mut u32).cast::<c_void>(),
            (&mut input_rowbytes_param as *mut u32).cast::<c_void>(),
            (&mut output_rowbytes_param as *mut u32).cast::<c_void>(),
            (&mut mode_param as *mut u32).cast::<c_void>(),
        ];

        let block_x = 16u32;
        let block_y = 16u32;
        let grid_x = width.div_ceil(block_x);
        let grid_y = height.div_ceil(block_y);
        check(
            (driver.cu_launch_kernel)(
                driver.function,
                grid_x,
                grid_y,
                1,
                block_x,
                block_y,
                1,
                0,
                stream,
                params.as_mut_ptr(),
                std::ptr::null_mut(),
            ),
            "cuLaunchKernel",
        )?;
        check(
            (driver.cu_stream_synchronize)(stream),
            "cuStreamSynchronize",
        )?;
    }
    Ok(())
}

struct CudaDriver {
    _lib: Library,
    cu_init: CuInit,
    cu_ctx_set_current: CuCtxSetCurrent,
    cu_launch_kernel: CuLaunchKernel,
    cu_stream_synchronize: CuStreamSynchronize,
    _module: CUmodule,
    function: CUfunction,
}

unsafe impl Send for CudaDriver {}
unsafe impl Sync for CudaDriver {}

fn driver() -> Result<&'static CudaDriver, String> {
    static DRIVER: OnceLock<Result<CudaDriver, String>> = OnceLock::new();
    DRIVER
        .get_or_init(load_driver)
        .as_ref()
        .map_err(|err| err.clone())
}

fn load_driver() -> Result<CudaDriver, String> {
    unsafe {
        let lib = Library::new("nvcuda.dll").map_err(|err| err.to_string())?;
        let cu_init = load::<CuInit>(&lib, b"cuInit\0")?;
        let cu_ctx_set_current = load::<CuCtxSetCurrent>(&lib, b"cuCtxSetCurrent\0")?;
        let cu_module_load_data = load::<CuModuleLoadData>(&lib, b"cuModuleLoadData\0")?;
        let cu_module_get_function = load::<CuModuleGetFunction>(&lib, b"cuModuleGetFunction\0")?;
        let cu_launch_kernel = load::<CuLaunchKernel>(&lib, b"cuLaunchKernel\0")?;
        let cu_stream_synchronize = load::<CuStreamSynchronize>(&lib, b"cuStreamSynchronize\0")?;

        check(cu_init(0), "cuInit")?;

        let ptx = CString::new(PROCESS_BGRA128_PTX).map_err(|err| err.to_string())?;
        let mut module = std::ptr::null_mut();
        check(
            cu_module_load_data(&mut module, ptx.as_ptr().cast::<c_void>()),
            "cuModuleLoadData",
        )?;

        let name = CString::new("process_bgra128").map_err(|err| err.to_string())?;
        let mut function = std::ptr::null_mut();
        check(
            cu_module_get_function(&mut function, module, name.as_ptr()),
            "cuModuleGetFunction",
        )?;

        Ok(CudaDriver {
            _lib: lib,
            cu_init,
            cu_ctx_set_current,
            cu_launch_kernel,
            cu_stream_synchronize,
            _module: module,
            function,
        })
    }
}

unsafe fn load<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, String> {
    let symbol: Symbol<T> = unsafe { lib.get(name).map_err(|err| err.to_string())? };
    Ok(*symbol)
}

fn check(result: CUresult, label: &str) -> Result<(), String> {
    if result == 0 {
        Ok(())
    } else {
        Err(format!("{label} returned CUDA error {result}"))
    }
}

const PROCESS_BGRA128_PTX: &str = r#"
.version 7.0
.target sm_52
.address_size 64

.visible .entry process_bgra128(
    .param .u64 input,
    .param .u64 output,
    .param .u32 width,
    .param .u32 height,
    .param .u32 input_rowbytes,
    .param .u32 output_rowbytes,
    .param .u32 mode
)
{
    .reg .pred %p<4>;
    .reg .b32 %r<13>;
    .reg .b64 %rd<7>;
    .reg .f32 %f<4>;

    ld.param.u64 %rd1, [input];
    ld.param.u64 %rd2, [output];
    ld.param.u32 %r1, [width];
    ld.param.u32 %r2, [height];
    ld.param.u32 %r3, [input_rowbytes];
    ld.param.u32 %r4, [output_rowbytes];
    ld.param.u32 %r5, [mode];

    mov.u32 %r6, %ctaid.x;
    mov.u32 %r7, %ntid.x;
    mov.u32 %r8, %tid.x;
    mad.lo.u32 %r9, %r6, %r7, %r8;

    mov.u32 %r6, %ctaid.y;
    mov.u32 %r7, %ntid.y;
    mov.u32 %r8, %tid.y;
    mad.lo.u32 %r10, %r6, %r7, %r8;

    setp.ge.u32 %p1, %r9, %r1;
    setp.ge.u32 %p2, %r10, %r2;
    or.pred %p3, %p1, %p2;
    @%p3 bra DONE;

    mul.lo.u32 %r11, %r10, %r3;
    shl.b32 %r12, %r9, 4;
    add.u32 %r11, %r11, %r12;
    cvt.u64.u32 %rd3, %r11;
    add.u64 %rd4, %rd1, %rd3;

    mul.lo.u32 %r11, %r10, %r4;
    add.u32 %r11, %r11, %r12;
    cvt.u64.u32 %rd5, %r11;
    add.u64 %rd6, %rd2, %rd5;

    ld.global.v4.f32 {%f0, %f1, %f2, %f3}, [%rd4];

    setp.ne.u32 %p1, %r5, 1;
    @%p1 bra STORE;
    sub.rn.f32 %f0, 1f3f800000, %f0;
    sub.rn.f32 %f1, 1f3f800000, %f1;
    sub.rn.f32 %f2, 1f3f800000, %f2;

STORE:
    st.global.v4.f32 [%rd6], {%f0, %f1, %f2, %f3};

DONE:
    ret;
}
"#;
