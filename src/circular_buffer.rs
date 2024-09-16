pub(crate) struct CircularBuffer {
    pub(crate) circular_buffer: Vec<f32>,

    pub(crate) samples_written: usize,
}

impl CircularBuffer {
    pub(crate) fn new(max_size: usize) -> Self {
        let mut circular_buffer = Vec::with_capacity(max_size);
        for _ in 0..max_size {
            circular_buffer.push(0.0);
        }
        CircularBuffer { circular_buffer, samples_written: 0 }
    }

    pub(crate) fn add_samples(&mut self, samples: &[f32]) {
        // In circular mode, overwrite old data if necessary
        for &sample in samples {
            self.samples_written += 1;

            let write_pos = self.write_pos();
            self.circular_buffer[write_pos] = sample;
        }
    }

    fn write_pos(&self) -> usize {
        self.samples_written % self.circular_buffer.len()
    }

    pub(crate) fn get_audio(&self, pad: bool) -> Vec<f32> {
        let mut audio = vec![];

        // Copy the contents of the circular buffer to the static buffer
        if (self.samples_written >= self.circular_buffer.len()) || pad == true {
            audio.extend_from_slice(&self.circular_buffer[self.write_pos()..]);
        }

        audio.extend_from_slice(&self.circular_buffer[..self.write_pos()]);

        audio
    }
}
