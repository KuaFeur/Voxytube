ALTER TABLE websub_subscriptions
    DROP CONSTRAINT IF EXISTS websub_subscriptions_state_check;

ALTER TABLE websub_subscriptions
    ADD CONSTRAINT websub_subscriptions_state_check
    CHECK (state IN ('pending', 'active', 'unsubscribing', 'expired', 'failed'));
