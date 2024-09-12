pub(crate) struct CircularBuffer {
    pub(crate) circular_buffer: Vec<f32>,
    pub(crate) static_buffer: Vec<f32>,
    pub(crate) max_size: usize,
    pub(crate) write_pos: usize,
    pub(crate) current_size: usize,
    pub(crate) is_static_mode: bool,
}

impl CircularBuffer {
    pub(crate) fn new(max_size: usize) -> Self {
        CircularBuffer {
            circular_buffer: Vec::with_capacity(max_size),
            static_buffer: Vec::new(), // Start with an empty static buffer
            max_size,
            write_pos: 0,
            current_size: 0,
            is_static_mode: false,
        }
    }

    pub(crate) fn add_samples(&mut self, samples: &[f32]) {
        if self.is_static_mode {
            // In static mode, add samples to the static buffer
            self.static_buffer.extend_from_slice(samples);
            self.current_size += samples.len();
        } else {
            // In circular mode, overwrite old data if necessary
            for &sample in samples {
                if self.current_size < self.max_size {
                    self.circular_buffer.push(sample);
                    self.current_size += 1;
                } else {
                    self.circular_buffer[self.write_pos] = sample;
                }
                self.write_pos = (self.write_pos + 1) % self.max_size;
            }
        }
    }

    pub(crate) fn start_static_mode(&mut self) {
        self.is_static_mode = true;

        println!("Transitioning to static mode");

        // Copy the contents of the circular buffer to the static buffer
        if self.current_size < self.max_size {
            self.static_buffer.extend_from_slice(&self.circular_buffer[..self.current_size]);
        } else {
            self.static_buffer.extend_from_slice(&self.circular_buffer[self.write_pos..]);
            self.static_buffer.extend_from_slice(&self.circular_buffer[..self.write_pos]);
        }
    }

    // Method to get the current samples for plotting (whether static or circular)
    pub(crate) fn get_samples_for_plot(&self) -> Vec<f32> {
        if self.is_static_mode {
            self.static_buffer.clone()
        } else if self.current_size < self.max_size {
            self.circular_buffer.clone()
        } else {
            let mut plot_samples = Vec::with_capacity(self.max_size);
            plot_samples.extend_from_slice(&self.circular_buffer[self.write_pos..]);
            plot_samples.extend_from_slice(&self.circular_buffer[..self.write_pos]);
            plot_samples
        }
    }
}
