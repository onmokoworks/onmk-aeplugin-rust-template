# AE GPU world と wgpu-hal external import 調査

更新時点: wgpu 26 / wgpu-hal 26

## 結論

`wgpu-hal`によるzero-copyは実装可能。ただし「同じコードでCUDA / DirectX /
Metalを自動吸収する仕組み」ではなく、AEが選択したGPU frameworkごとのnative
resourceを、同一native device上のwgpuへ安全に登録するbackend別実装になる。

現時点の優先順位:

1. CUDA official GPU path: 現在のWindows/NVIDIA機で動作確認済み。実用backend。
2. Metal buffer import: Apple Siliconで最も成立しやすいwgpu-hal研究経路。
3. D3D12 buffer import: wrapping自体は成功。AE deviceとwgpu deviceの同一性が壁。

## AE側で確定している契約

- GPU SmartRender中のCPU `PF_LayerDef.data`はnull。
- GPU worldは`PF_PixelFormat_GPU_BGRA128`で、1 pixelあたり16 byteのfloat BGRA。
- `GetGPUWorldData`、`rowbytes`、`GetGPUWorldSize`を組み合わせてpitch付きbufferとして扱う。
- device/context/command queueは`PF_GPUDeviceSuite::GetDeviceInfo`から取得する。
- full GPU effectのrender callback中はexclusive accessが保持される。
- CUDAはDriver APIの利用がAdobe推奨。

参考:

- [Adobe command selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)
- [Adobe GPU build instructions](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/)
- [PF_EffectWorld](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/)

## wgpu-hal 26で使える入口

D3D12:

```text
ID3D12Resource*
  -> retain / GetDesc / GetDevice
  -> wgpu_hal::dx12::Device::buffer_from_raw
  -> wgpu::Device::create_buffer_from_hal
  -> wgpu::Buffer
```

Metal:

```text
MTLBuffer
  -> retain / length / MTLDevice identity check
  -> wgpu_hal::metal::Device::buffer_from_raw
  -> wgpu::Device::create_buffer_from_hal
  -> wgpu::Buffer
```

`create_buffer_from_hal`の安全条件は、raw bufferがその`wgpu::Device`内部のnative
deviceから作られていること、descriptorと実resourceが一致すること、初期化済みで
あること。

参考:

- [wgpu Device::create_buffer_from_hal](https://docs.rs/wgpu/26.0.1/wgpu/struct.Device.html#method.create_buffer_from_hal)
- [wgpu-hal crate](https://docs.rs/wgpu-hal/26.0.6/wgpu_hal/)

## Windows / D3D12

実装済みのpreflight:

- backendがD3D12
- handleがnon-null `ID3D12Resource*`
- resource dimensionがBUFFER
- allocation widthが`rowbytes * height`以上
- resourceの`ID3D12Device`とwgpu内部の`ID3D12Device`が同一COM object
- access、ownership、synchronizationの方針が明示済み

AEなしのsmoke testでは、同一D3D12 deviceから作った外部resourceをwrapし、WGSL
computeで値を書き換え、readbackまで成功した。

残る問題はdevice生成。普通に`wgpu::Instance::request_adapter/request_device`すると、
同じGPU adapterでもAEとは別の`ID3D12Device`が作られる可能性が高い。D3D12 resource
は作成元deviceに属するため、adapterが同じだけでは不足する。

候補は次の3つ:

1. AEがDirectXを選んだ実機で、偶然ではなくdevice identityをprobeする。
2. `wgpu-hal`を小さくforkし、DX12のhost-owned device/queue constructorを追加する。
3. AE resourceがshared handle対応なら、wgpu側deviceへopenし直す。ただしAE SDKは
   shared handleを保証していない。

現状のupstream `wgpu-hal 26`にはMetalの`device_from_raw` / `queue_from_raw`相当は
あるが、DX12には公開された同等constructorがない。この非対称性が実装難易度の差。

## macOS / Metal

Metalは通常のwgpu adapterがAEと同じsystem `MTLDevice` objectを参照できる可能性が
高い。実装はpointer identityを確認してから`MTLBuffer`をretainしてwrapする。

`aarch64-apple-darwin`向けに以下がcompile済み:

- Metal external context
- same-device check
- MTLBuffer length check
- retainして`buffer_from_raw`
- `create_buffer_from_hal`
- same-device smoke test

Mac実機で確認する項目:

- `devicePV`が`MTLDevice`か
- `command_queuePV`が`MTLCommandQueue`か
- `GetGPUWorldData`が`MTLBuffer` objectか、buffer contents pointerか
- 通常のwgpu deviceとAEの`MTLDevice` pointerが一致するか
- AE queue上のinput準備完了をどの方法で保証するか

## 同期と寿命

zero-copyで最も危険なのはpointer変換ではなく同期。

入力側:

- AEがinput worldへの書き込みを完了してからwgpuが読む。
- 別command queueなら明示的なevent/fence、またはhost callback契約が必要。

出力側:

- wgpu submit完了前にAEへcallbackを返さない。
- 初期実装は`device.poll(PollType::Wait)`で完了待ちし、正しさを優先する。

D3D12 resource state:

- import時のstateを推測しない。
- AEが期待する返却stateをprobeまたはSDK sampleで確定する。
- stateが不明な間は実AE worldへのhot importを有効化しない。

寿命:

- COM/Objective-C objectはborrowしたraw pointerを直接所有扱いしない。
- D3D12はCOM clone、Metalはretainしたobjectをwgpu-hal wrapperへ渡す。
- wgpu objectはAE worldの有効期間内にdropし、GPU work完了後に返却する。

## 実装配置

```text
tracks/02_native_official_gpu_probe
  AE selector / device / frameをNativeRenderJobへ変換
  CUDA native backend

tracks/03_wgpu_hal_external_import
  common preflight
  D3D12 buffer importer
  Metal buffer importer
  pitch-aware BGRA128 WGSL compute
  AEなしのbackend smoke tests
```

実機AEが使えるようになったら、最初に追加するのはeffect処理ではなく「raw handleの
object種別、device identity、同期点」を記録するprobe。そこが確定したbackendだけを
hot importへ進める。
