pub struct MotorValueFilter {
    alpha: f32,              // EMA smoothing
    threshold: f32,          // Deadband threshold
    zero_threshold: f32,     // Wat beschouwen we als "0"
    filtered: Option<f32>,
    last_sent: Option<f32>,
}

impl MotorValueFilter {
    pub fn new(alpha: f32, threshold: f32, zero_threshold: f32) -> Self {
        Self {
            alpha,
            threshold,
            zero_threshold,
            filtered: None,
            last_sent: None,
        }
    }
    
    pub fn update(&mut self, raw_value: f32) -> Option<f32> {
        // EMA smoothing
        let smoothed = match self.filtered {
            None => raw_value,
            Some(old) => old * (1.0 - self.alpha) + raw_value * self.alpha,
        };
        self.filtered = Some(smoothed);
        
        let is_zero = smoothed.abs() < self.zero_threshold;
        let was_nonzero = self.last_sent
            .map(|v| v.abs() >= self.zero_threshold)
            .unwrap_or(false);
        
        // Altijd versturen als:
        // 1. Eerste waarde
        // 2. Overschrijdt deadband threshold
        // 3. Transitie naar 0 (zero-crossing)
        let should_send = match self.last_sent {
            None => true,
            Some(last) => {
                (smoothed - last).abs() > self.threshold  // Grote verandering
                || (is_zero && was_nonzero)                // Transitie naar 0
            }
        };
        
        if should_send {
            let value_to_send = if is_zero { 0.0 } else { smoothed };
            self.last_sent = Some(value_to_send);
            Some(value_to_send)
        } else {
            None
        }
    }
}