use sqlx::PgPool;
use time::PrimitiveDateTime;
use uuid::Uuid;

/// Module and schema representing a refund.
///
/// A refund is always tied to a specific payment record, but it is possible
/// to make partial refunds (i.e. refund less than the total payment amount).
/// In the same vein, it is possible to apply several refunds against the same
/// payment record, the but sum of all refunded amounts for a given payment can
/// never surpass the original payment amount.
///
/// If a refund is persisted in the database, it is considered effective: the
/// bank's client will have the money credited to their account.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Refund {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub amount: i32,
    pub inserted_at: PrimitiveDateTime,
    pub updated_at: PrimitiveDateTime,
}

pub async fn insert(pool: &PgPool, payment_id: Uuid, amount: i32) -> Result<Uuid, sqlx::Error> {
    sqlx::query!(
        r#"
            INSERT INTO refunds ( payment_id, amount )
            VALUES ( $1, $2 )
            RETURNING id
        "#,
        payment_id,
        amount,
    )
    .fetch_one(pool)
    .await
    .map(|record| record.id)
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Refund, sqlx::Error> {
    sqlx::query_as!(
        Refund,
        r#"
            SELECT id, payment_id, amount, inserted_at, updated_at FROM refunds
            WHERE id = $1
        "#,
        id
    )
    .fetch_one(pool)
    .await
}

pub async fn checked_insert(
    pool: &PgPool,
    payment_id: Uuid,
    refund_amount: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query!(
        r#"
          INSERT into refunds ( payment_id, amount )
          SELECT $1, $2
          WHERE EXISTS (
            SELECT ( t2.amount - SUM(t1.amount) ) 
            FROM refunds t1 
            JOIN payments t2 on t1.payment_id = t2.id 
            WHERE t1.payment_id = $1 
            GROUP BY t1.payment_id, t2.amount
            HAVING t2.amount - SUM(t1.amount) >= $2::integer
          ) OR (
            NOT EXISTS (
              SELECT * FROM refunds WHERE payment_id = $1
            )
            AND EXISTS (
              SELECT * FROM payments WHERE id = $1 AND amount >= $2
            )
          )
          RETURNING id
        "#,
        payment_id,
        refund_amount
    )
    .fetch_optional(pool)
    .await
    .map(|record| record.map(|r| r.id))
}

#[cfg(test)]
pub mod tests {

    use super::*;
    use crate::bank::payments::Payment;

    pub const REFUND_AMOUNT: i32 = 42;

    impl Refund {
        pub async fn new_test(pool: &PgPool) -> Result<Refund, sqlx::Error> {
            let payment = Payment::new_test(pool).await?;

            let id = insert(pool, payment.id, REFUND_AMOUNT).await?;

            get(pool, id).await
        }
    }

    #[tokio::test]
    async fn test_refund() {
        let pool = crate::pg_pool()
            .await
            .expect("failed to connect to postgres");

        let refund = Refund::new_test(&pool)
            .await
            .expect("failed to create refund");

        assert_eq!(refund.amount, REFUND_AMOUNT);
    }
}
