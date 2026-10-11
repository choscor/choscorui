//! The visual state a recipe sees: a small set of flags, never input.

/// Flags that change how a control looks. Recipes read only these, the
/// variant, the size, and the tokens.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VisualState(u16);

impl VisualState {
    pub const NONE: Self = Self(0);
    pub const HOVERED: Self = Self(1 << 0);
    pub const PRESSED: Self = Self(1 << 1);
    /// Focus that came from the keyboard: the focus ring shows.
    pub const FOCUS_VISIBLE: Self = Self(1 << 2);
    pub const DISABLED: Self = Self(1 << 3);
    pub const INVALID: Self = Self(1 << 4);
    pub const CHECKED: Self = Self(1 << 5);
    pub const INDETERMINATE: Self = Self(1 << 6);
    pub const SELECTED: Self = Self(1 << 7);
    pub const OPEN: Self = Self(1 << 8);
    pub const HIGHLIGHTED: Self = Self(1 << 9);
    /// Focus from any source, for styles that do not need the ring.
    pub const FOCUSED: Self = Self(1 << 10);

    pub const ALL: [(VisualState, &'static str); 11] = [
        (Self::HOVERED, "hovered"),
        (Self::PRESSED, "pressed"),
        (Self::FOCUS_VISIBLE, "focus-visible"),
        (Self::DISABLED, "disabled"),
        (Self::INVALID, "invalid"),
        (Self::CHECKED, "checked"),
        (Self::INDETERMINATE, "indeterminate"),
        (Self::SELECTED, "selected"),
        (Self::OPEN, "open"),
        (Self::HIGHLIGHTED, "highlighted"),
        (Self::FOCUSED, "focused"),
    ];

    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0 && flag.0 != 0
    }

    #[must_use]
    pub const fn with(self, flag: Self) -> Self {
        Self(self.0 | flag.0)
    }

    #[must_use]
    pub const fn without(self, flag: Self) -> Self {
        Self(self.0 & !flag.0)
    }

    /// Adds or removes `flag`.
    #[must_use]
    pub const fn set(self, flag: Self, on: bool) -> Self {
        if on {
            self.with(flag)
        } else {
            self.without(flag)
        }
    }

    pub const fn hovered(self) -> bool {
        self.contains(Self::HOVERED)
    }
    pub const fn pressed(self) -> bool {
        self.contains(Self::PRESSED)
    }
    pub const fn focus_visible(self) -> bool {
        self.contains(Self::FOCUS_VISIBLE)
    }
    pub const fn disabled(self) -> bool {
        self.contains(Self::DISABLED)
    }
    pub const fn invalid(self) -> bool {
        self.contains(Self::INVALID)
    }
    pub const fn checked(self) -> bool {
        self.contains(Self::CHECKED)
    }
    pub const fn indeterminate(self) -> bool {
        self.contains(Self::INDETERMINATE)
    }
    pub const fn selected(self) -> bool {
        self.contains(Self::SELECTED)
    }
    pub const fn open(self) -> bool {
        self.contains(Self::OPEN)
    }
    pub const fn highlighted(self) -> bool {
        self.contains(Self::HIGHLIGHTED)
    }
    pub const fn focused(self) -> bool {
        self.contains(Self::FOCUSED)
    }

    /// Hover applies only while enabled; the disabled state has no pointer
    /// interaction.
    pub const fn interactive_hover(self) -> bool {
        self.hovered() && !self.disabled()
    }
}

impl std::fmt::Display for VisualState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for (flag, name) in Self::ALL {
            if self.contains(flag) {
                if !first {
                    f.write_str("+")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        if first {
            f.write_str("rest")?;
        }
        Ok(())
    }
}
