use wmidi::Channel;

pub struct MpeVoiceAllocator {
    // 0 = Free, 1 = Taken
    // We treat index 0 as Ch1 (Master), usually we don't alloc it for notes.
    // Indices 1..15 as Ch2..Ch16.
    usage_mask: u16,
}

impl MpeVoiceAllocator {
    pub const fn new() -> Self {
        Self { usage_mask: 0 }
    }

    /// Try to allocate a channel from Ch2 to Ch16.
    pub fn alloc(&mut self) -> Option<Channel> {
        // Iterate over indices 1 to 15 (Channels 2 to 16)
        for i in 1..16 {
            let mask = 1 << i;
            if (self.usage_mask & mask) == 0 {
                self.usage_mask |= mask;
                return Self::index_to_channel(i);
            }
        }
        None
    }

    pub fn free(&mut self, channel: Channel) {
        let i = Self::channel_to_index(channel);
        if i > 0 {
            // Don't touch Ch1 if we mistakenly got it
            self.usage_mask &= !(1 << i);
        }
    }

    fn index_to_channel(i: usize) -> Option<Channel> {
        match i {
            0 => Some(Channel::Ch1),
            1 => Some(Channel::Ch2),
            2 => Some(Channel::Ch3),
            3 => Some(Channel::Ch4),
            4 => Some(Channel::Ch5),
            5 => Some(Channel::Ch6),
            6 => Some(Channel::Ch7),
            7 => Some(Channel::Ch8),
            8 => Some(Channel::Ch9),
            9 => Some(Channel::Ch10),
            10 => Some(Channel::Ch11),
            11 => Some(Channel::Ch12),
            12 => Some(Channel::Ch13),
            13 => Some(Channel::Ch14),
            14 => Some(Channel::Ch15),
            15 => Some(Channel::Ch16),
            _ => None,
        }
    }

    fn channel_to_index(c: Channel) -> usize {
        match c {
            Channel::Ch1 => 0,
            Channel::Ch2 => 1,
            Channel::Ch3 => 2,
            Channel::Ch4 => 3,
            Channel::Ch5 => 4,
            Channel::Ch6 => 5,
            Channel::Ch7 => 6,
            Channel::Ch8 => 7,
            Channel::Ch9 => 8,
            Channel::Ch10 => 9,
            Channel::Ch11 => 10,
            Channel::Ch12 => 11,
            Channel::Ch13 => 12,
            Channel::Ch14 => 13,
            Channel::Ch15 => 14,
            Channel::Ch16 => 15,
        }
    }
}
