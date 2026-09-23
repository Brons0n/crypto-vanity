use super::{GpuBackend, LANES, PARAM_WORDS, STEPS};
use crate::pattern::Pattern;
use anyhow::{anyhow, bail, Result};
use objc2::{
    rc::{autoreleasepool, Retained},
    runtime::ProtocolObject,
};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandEncoder, MTLCommandQueue,
    MTLComputeCommandEncoder, MTLComputePipelineState, MTLCopyAllDevices, MTLDevice, MTLLibrary,
    MTLResourceOptions, MTLSize,
};

pub struct Metal {
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    points: Retained<ProtocolObject<dyn MTLBuffer>>,
    params: Retained<ProtocolObject<dyn MTLBuffer>>,
    hits: Retained<ProtocolObject<dyn MTLBuffer>>,
    name: String,
}

impl Metal {
    pub fn new() -> Result<Self> {
        autoreleasepool(|_| {
            let mut failures = Vec::new();
            for device in MTLCopyAllDevices() {
                match Self::on_device(&device) {
                    Ok(mut gpu) => match super::self_test(&mut gpu) {
                        Ok(()) => return Ok(gpu),
                        Err(e) => failures.push(e.to_string()),
                    },
                    Err(e) => failures.push(e.to_string()),
                }
            }
            bail!(
                "no usable Metal GPU: {}",
                if failures.is_empty() {
                    "no devices found".to_owned()
                } else {
                    failures.join("; ")
                }
            )
        })
    }

    fn on_device(device: &ProtocolObject<dyn MTLDevice>) -> Result<Self> {
        let source =
            NSString::from_str(&[include_str!("common.h"), include_str!("kernel.metal")].concat());
        let library = device
            .newLibraryWithSource_options_error(&source, None)
            .map_err(|e| anyhow!("Metal shader compilation failed: {e}"))?;
        let function = library
            .newFunctionWithName(&NSString::from_str("search"))
            .ok_or_else(|| anyhow!("Metal search function missing"))?;
        let pipeline = device
            .newComputePipelineStateWithFunction_error(&function)
            .map_err(|e| anyhow!("Metal pipeline failed: {e}"))?;
        let queue = device
            .newCommandQueue()
            .ok_or_else(|| anyhow!("Metal command queue allocation failed"))?;
        let allocate = |words| {
            device
                .newBufferWithLength_options(words * 4, MTLResourceOptions::StorageModeShared)
                .ok_or_else(|| anyhow!("Metal shared buffer allocation failed"))
        };
        Ok(Self {
            queue,
            pipeline,
            points: allocate(LANES * 24)?,
            params: allocate(PARAM_WORDS)?,
            hits: allocate(LANES + 1)?,
            name: format!("Metal / {}", device.name()),
        })
    }
}

impl GpuBackend for Metal {
    fn name(&self) -> &str {
        &self.name
    }
    fn init(&mut self, points: &[u32], pattern: &Pattern) -> Result<()> {
        if points.len() != LANES * 24 {
            bail!("invalid GPU point buffer length");
        }
        // SAFETY: buffers have the exact allocation sizes above, are CPU-visible,
        // and no GPU command is outstanding. Only public points/criteria are copied.
        unsafe {
            std::ptr::copy_nonoverlapping(
                points.as_ptr(),
                self.points.contents().as_ptr().cast::<u32>(),
                points.len(),
            );
            let params = super::parameters(pattern)?;
            std::ptr::copy_nonoverlapping(
                params.as_ptr(),
                self.params.contents().as_ptr().cast::<u32>(),
                params.len(),
            );
        }
        Ok(())
    }
    fn dispatch_batch(&mut self) -> Result<u64> {
        autoreleasepool(|_| {
            // Shared storage: on Apple Silicon both processors access the same
            // physical allocation; no staging buffers or blit copies are needed.
            unsafe {
                self.hits
                    .contents()
                    .as_ptr()
                    .cast::<u32>()
                    .add(LANES)
                    .write(0);
            }
            let command = self
                .queue
                .commandBuffer()
                .ok_or_else(|| anyhow!("Metal command allocation failed"))?;
            let encoder = command
                .computeCommandEncoder()
                .ok_or_else(|| anyhow!("Metal encoder allocation failed"))?;
            encoder.setComputePipelineState(&self.pipeline);
            // SAFETY: buffer indices match kernel.metal; all buffers outlive GPU use.
            unsafe {
                encoder.setBuffer_offset_atIndex(Some(&self.points), 0, 0);
                encoder.setBuffer_offset_atIndex(Some(&self.params), 0, 1);
                encoder.setBuffer_offset_atIndex(Some(&self.hits), 0, 2);
            }
            // Power-of-two workgroups divide LANES even on older AMD devices.
            let max = self.pipeline.maxTotalThreadsPerThreadgroup().min(32);
            if max == 0 {
                bail!("Metal reports no supported compute threads");
            }
            let width = 1usize << (usize::BITS - 1 - max.leading_zeros());
            encoder.dispatchThreadgroups_threadsPerThreadgroup(
                MTLSize {
                    width: LANES / width,
                    height: 1,
                    depth: 1,
                },
                MTLSize {
                    width,
                    height: 1,
                    depth: 1,
                },
            );
            encoder.endEncoding();
            command.commit();
            command.waitUntilCompleted();
            if command.status() != MTLCommandBufferStatus::Completed {
                bail!("Metal dispatch failed: {:?}", command.error());
            }
            Ok(LANES as u64 * STEPS)
        })
    }
    fn retrieve_match(&mut self) -> Result<Option<(usize, u32)>> {
        // SAFETY: dispatch waits for completion before the CPU reads shared storage.
        let hits = unsafe {
            std::slice::from_raw_parts(self.hits.contents().as_ptr().cast::<u32>(), LANES + 1)
        };
        if hits[LANES] == 0 {
            return Ok(None);
        }
        hits[..LANES]
            .iter()
            .enumerate()
            .find(|(_, hit)| **hit != 0)
            .map(|(lane, hit)| Some((lane, hit - 1)))
            .ok_or_else(|| anyhow!("Metal inconsistent match status"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires macOS with a Metal GPU"]
    fn hardware_known_addresses() {
        let mut backend = super::Metal::new().unwrap();
        crate::gpu::differential_test(&mut backend).unwrap();
    }
}
