#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl ControlSize {
    pub(crate) const fn button_class(self) -> &'static str {
        match self {
            Self::Small => "min-h-7.5 rounded-md px-2.5 text-xs",
            Self::Medium => "min-h-8.5 rounded-lg px-3.5 text-[13px]",
            Self::Large => "min-h-10 rounded-lg px-4 text-sm",
        }
    }

    pub(crate) const fn icon_button_class(self) -> &'static str {
        match self {
            Self::Small => "size-7.25 min-w-7.25 rounded-md",
            Self::Medium => "size-8.5 min-w-8.5 rounded-lg",
            Self::Large => "size-10 min-w-10 rounded-lg",
        }
    }

    pub(crate) const fn icon_size(self) -> u32 {
        match self {
            Self::Small => 14,
            Self::Medium => 16,
            Self::Large => 18,
        }
    }

    pub(crate) const fn input_class(self) -> &'static str {
        match self {
            Self::Small => "min-h-7.5 rounded-md px-2 py-1.25 text-xs",
            Self::Medium => "min-h-8.5 rounded-md px-2.75 py-2 text-[13px]",
            Self::Large => "min-h-10 rounded-lg px-3 py-2.5 text-sm",
        }
    }

    pub(crate) const fn text_area_class(self) -> &'static str {
        match self {
            Self::Small => "rounded-md px-2 py-1.5 text-xs",
            Self::Medium => "rounded-md px-2.75 py-2 text-[13px]",
            Self::Large => "rounded-lg px-3 py-2.5 text-sm",
        }
    }
}
