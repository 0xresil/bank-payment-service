CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE TYPE Status AS ENUM ('Processing', 'Approved', 'Declined', 'Failed');

CREATE TABLE payments (
    id uuid default uuid_generate_v4() PRIMARY KEY UNIQUE,
    amount integer NOT NULL,
    card_number character varying(255) NOT NULL UNIQUE,
    status Status  NOT NULL,
    inserted_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);
-- -- CREATE UNIQUE INDEX payments_pkey ON payments(id uuid_ops);
-- CREATE UNIQUE INDEX payments_id_index ON payments(id uuid_ops);
-- CREATE UNIQUE INDEX payments_card_number_index ON payments(card_number text_ops);

CREATE TABLE refunds (
    id uuid default uuid_generate_v4() PRIMARY KEY UNIQUE,
    payment_id uuid REFERENCES payments(id) NOT NULL,
    amount integer NOT NULL,
    inserted_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);

-- CREATE UNIQUE INDEX refunds_pkey ON refunds(id uuid_ops);
-- CREATE UNIQUE INDEX refunds_id_index ON refunds(id uuid_ops);
-- CREATE INDEX refunds_payment_id_index ON refunds(payment_id uuid_ops);
