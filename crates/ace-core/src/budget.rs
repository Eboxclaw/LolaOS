//! Host-owned hard limits for one execution run.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub max_transitions: u32,
    pub max_model_calls: u32,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_tool_calls: u32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BudgetUsage {
    pub transitions: u32,
    pub model_calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetError {
    TransitionsExceeded,
    ModelCallsExceeded,
    InputTokensExceeded,
    OutputTokensExceeded,
    ToolCallsExceeded,
}

impl BudgetUsage {
    pub fn record_transition(&mut self, budget: &Budget) -> Result<(), BudgetError> {
        Self::increment(
            &mut self.transitions,
            budget.max_transitions,
            BudgetError::TransitionsExceeded,
        )
    }

    pub fn record_model_call(&mut self, budget: &Budget) -> Result<(), BudgetError> {
        Self::increment(
            &mut self.model_calls,
            budget.max_model_calls,
            BudgetError::ModelCallsExceeded,
        )
    }

    pub fn record_tool_call(&mut self, budget: &Budget) -> Result<(), BudgetError> {
        Self::increment(
            &mut self.tool_calls,
            budget.max_tool_calls,
            BudgetError::ToolCallsExceeded,
        )
    }

    pub fn record_input_tokens(&mut self, count: u64, budget: &Budget) -> Result<(), BudgetError> {
        let next = self.input_tokens.saturating_add(count);
        if next > budget.max_input_tokens {
            return Err(BudgetError::InputTokensExceeded);
        }
        self.input_tokens = next;
        Ok(())
    }

    pub fn record_output_tokens(&mut self, count: u64, budget: &Budget) -> Result<(), BudgetError> {
        let next = self.output_tokens.saturating_add(count);
        if next > budget.max_output_tokens {
            return Err(BudgetError::OutputTokensExceeded);
        }
        self.output_tokens = next;
        Ok(())
    }

    fn increment(value: &mut u32, limit: u32, error: BudgetError) -> Result<(), BudgetError> {
        let next = value.saturating_add(1);
        if next > limit {
            return Err(error);
        }
        *value = next;
        Ok(())
    }
}
