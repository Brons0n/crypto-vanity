use super::{GpuBackend, LANES, PARAM_WORDS, STEPS};
use crate::pattern::Pattern;
use anyhow::{anyhow, bail, Context as _, Result};
use opencl3::{
    command_queue::CommandQueue,
    context::Context,
    device::{get_all_devices, Device, CL_DEVICE_TYPE_GPU},
    kernel::Kernel,
    memory::{Buffer, CL_MEM_READ_ONLY, CL_MEM_READ_WRITE},
    program::Program,
    types::CL_BLOCKING,
};
use std::ptr;

pub struct OpenCl {
    queue: CommandQueue,
    kernel: Kernel,
    points: Buffer<u32>,
    params: Buffer<u32>,
    hits: Buffer<u32>,
    name: String,
}

impl OpenCl {
    pub fn new() -> Result<Self> {
        let devices = get_all_devices(CL_DEVICE_TYPE_GPU)
            .context("enumerating OpenCL GPUs (check the driver/ICD installation)")?;
        let mut failures = Vec::new();
        for id in devices {
            match Self::on_device(Device::new(id)) {
                Ok(mut gpu) => match super::self_test(&mut gpu) {
                    Ok(()) => return Ok(gpu),
                    Err(e) => failures.push(e.to_string()),
                },
                Err(e) => failures.push(e.to_string()),
            }
        }
        bail!(
            "no usable OpenCL GPU: {}",
            if failures.is_empty() {
                "no devices found".to_owned()
            } else {
                failures.join("; ")
            }
        )
    }
    fn on_device(device: Device) -> Result<Self> {
        let context = Context::from_device(&device)?;
        let queue = CommandQueue::create_default(&context, 0)?;
        let program = Program::create_and_build_from_source(
            &context,
            &[include_str!("common.h"), include_str!("kernel.cl")].concat(),
            "-cl-std=CL1.2",
        )
        .map_err(|e| anyhow!("OpenCL kernel build failed: {e}"))?;
        let kernel = Kernel::create(&program, "search")?;
        // SAFETY: fixed positive element counts, no host pointer; runtime owns storage.
        let (points, params, hits) = unsafe {
            (
                Buffer::create(&context, CL_MEM_READ_WRITE, LANES * 24, ptr::null_mut())?,
                Buffer::create(&context, CL_MEM_READ_ONLY, PARAM_WORDS, ptr::null_mut())?,
                Buffer::create(&context, CL_MEM_READ_WRITE, LANES + 1, ptr::null_mut())?,
            )
        };
        Ok(Self {
            queue,
            kernel,
            points,
            params,
            hits,
            name: format!("OpenCL / {}", device.name()?),
        })
    }
}

impl GpuBackend for OpenCl {
    fn name(&self) -> &str {
        &self.name
    }
    fn init(&mut self, points: &[u32], pattern: &Pattern) -> Result<()> {
        if points.len() != LANES * 24 {
            bail!("invalid GPU point buffer length");
        }
        // SAFETY: blocking copies, buffers sized to the complete slices; no in-flight dispatch.
        unsafe {
            self.queue
                .enqueue_write_buffer(&mut self.points, CL_BLOCKING, 0, points, &[])?;
            self.queue.enqueue_write_buffer(
                &mut self.params,
                CL_BLOCKING,
                0,
                &super::parameters(pattern)?,
                &[],
            )?;
            self.kernel.set_arg(0, &self.points)?;
            self.kernel.set_arg(1, &self.params)?;
            self.kernel.set_arg(2, &self.hits)?;
        }
        Ok(())
    }
    fn dispatch_batch(&mut self) -> Result<u64> {
        // SAFETY: argument buffers outlive the dispatch; 256 lanes match the allocations.
        // An in-order queue and a blocking read in retrieve_match enforce host/device ordering.
        unsafe {
            self.queue.enqueue_write_buffer(
                &mut self.hits,
                CL_BLOCKING,
                LANES * 4,
                &[0u32],
                &[],
            )?;
            self.queue.enqueue_nd_range_kernel(
                self.kernel.get(),
                1,
                ptr::null(),
                [LANES].as_ptr(),
                ptr::null(),
                &[],
            )?;
        }
        Ok(LANES as u64 * STEPS)
    }
    fn retrieve_match(&mut self) -> Result<Option<(usize, u32)>> {
        let mut status = [0u32];
        // SAFETY: reads are blocking and within allocated buffer bounds.
        unsafe {
            self.queue
                .enqueue_read_buffer(&self.hits, CL_BLOCKING, LANES * 4, &mut status, &[])?;
        }
        if status[0] == 0 {
            return Ok(None);
        }
        let mut hits = [0u32; LANES];
        unsafe {
            self.queue
                .enqueue_read_buffer(&self.hits, CL_BLOCKING, 0, &mut hits, &[])?;
        }
        hits.iter()
            .enumerate()
            .find(|(_, hit)| **hit != 0)
            .map(|(lane, hit)| Some((lane, hit - 1)))
            .ok_or_else(|| anyhow!("OpenCL inconsistent match status"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires an installed OpenCL GPU driver"]
    fn hardware_known_addresses() {
        let mut backend = super::OpenCl::new().unwrap();
        crate::gpu::differential_test(&mut backend).unwrap();
    }
}
