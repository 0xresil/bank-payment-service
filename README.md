# Code Challenge

You'll be implementing a basic bank JSON API with payments and refunds:

- `POST` to `/payments` to create a new payment
- `POST` to `/payments/PAYMENT_ID/refunds` to make a (potentially partial) refund against a payment

The bank will deploy this API into production, where all instances will connect to the same single Postgres DB instance.

When clients buy something on an e-commerce website, merchants will call the `/payments` API endpoint to request money from that person's bank account. If the bank approves the request, the sale will succeed. Sometimes, e.g. if it turns out some goods can't be shipped (e.g. out of stock), refunds must be issued to the customers by calling the `/payments/PAYMENT_ID/refunds` endpoint to credit money back to the customer's bank account.

The bank provides its clients with single-use credit cards: each time a `/payment` is attempted, a new credit card number must be used. And since these are intended for "everyday" purchases, the API won't have to deal with huge numbers (payment amounts will comfortably fit in an `integer` DB column).

Internally, the bank has a legacy "accounts service" that will verify that a given customer has sufficient funds for a given payment. For this exercise, this service has been stubbed out as the `Bank.Accounts.DummyService` but please bear in mind that the underlying implementation would be located within a remote microservice.

## Scaffolding

We've provided a barebones code repository in the hopes it will save you time. However, none of this is set in stone and don't feel constrained: if you want to rewrite the hole thing or just parts, don't let your dreams be dreams and go for it!

We've documented the API requirement below, but if you prefer to jump into the deep end of the pool, you can be off to the races with:

```sh
cargo test
```

Getting the failing tests to pass should help you get going, and perhaps you'll find the meagre documentation useful also: view it with `cargo doc --no-deps --open`.

# /payments

The payments endpoint handles removing money from the customer's bank account: this money will later be transferred to the merchant (through a process called settlement), but that won't concern us here.

Since this is dealing with money, the API only exposes endpoints to create new payments and view existing payments: it's not possible to (e.g.) edit or delete payments from the API.

## Accounts service

As mentioned, the bank has an "accounts service" which is hosted in a separate micro-service.

This service is used to determine whether a customer has sufficient funds on their account, by attempting to place a [hold](https://en.wikipedia.org/wiki/Authorization_hold) on the requested amount: while a hold is present for an amount of money, that amount is "locked" and can't be spent by the customer (the difference between their [current](https://en.wikipedia.org/wiki/Authorization_hold) and [actual](https://www.creditkarma.com/money/i/current-balance-vs-available-balance#available-balance-what-to-know) balances would be the amount being held). A hold MUST then be either released (e.g. because the purchase was canceled by either party), or withdrawn from the customer's account thereby reducing their balance (and deleting the associated hold).

The accounts service doesn't respond well to load: unnecessary requests to it are to be avoided.

For this challenge, the implementation has been abstracted away, however you can safely assume that calls to this remote service DO NOT fail: their implementation relies on persistent, retryable jobs.

## Creation

`POST`ing to `/api/payments` with valid data should create a new payment. Valid data is defined as follows:

- `amount`: a positive `integer` value representing the monetary amount that is requested from the client's account. It represents the amount in the EUR currency, in cents: a EUR 10.45 purchase would be encoded as an amount of `1045`.
- `card_number`: a 15-digit numerical-only `string` value containing the single-use credit card number to use for the purchase. This value is expected to be unique, and there's no need to consider this a particularly sensitive information as every card number can be assumed to no longer be usable as soon as it hits an API endpoint: the card number can be logged, stored in clear in the DB, etc.

The above attributes must be wrapped within a `payment` attribute:

```
{
  "payment": {
    "amount": 1045,
    "card_number": "123451234512345"
  }
}
```

If the provided attributes are valid and the payment has been created, a `201` HTTP status response will be returned along with the created payment in the body's "data":

```
{
  "data": {
    "id": "9decbf6d-c470-4a1f-ae7b-8fb2a39db318",
    "amount": 1045,
    "card_number": "123451234512345",
    "status": "approved"
  }
}
```

As you can see, the API will set an `id` value and the response will contain a "approved" status in addition to the 201 HTTP status.

A "show" endpoint is also exposed by the API, and will return the above response if a GET request is made to `/api/payments/9decbf6d-c470-4a1f-ae7b-8fb2a39db318`. It's presence is mainly for convenience, as it's implementation won't be part of this challenge.

### Unhappy paths

Invariants:

- there is at most one payment per `card_number`: payment creation requests for a given `card_number` already associated with a payment record should return a 422 status.

Unhappy paths originating from an unhappy response from the accounts service should respond with a body containing the same data as a successful response (see above), but with a differing payment "status" value and HTTP status, as follows:

- `insufficient_funds`: `402 Payment required` and payment status "declined"
- `invalid_account_number`: `403 Forbidden` and payment status "declined"
- `service_unavailable`: `503 Service unavailable` and payment status "failed"
- `internal_error`: `500 Internal error` and payment status "failed"

Sadly, the "accounts service" mentioned above is really fragile. We've had issues in the past where it was unable to handle the load, so we do our best to not send unnecessary requests and the following cases should NOT contact the accounts API:

- payment requests for negative amounts should return a 400 response
- payment requests for 0 should return a 204 response
- invalid card formats should return a 422 response

For each of these 3 cases, whether or not a "payment" record is persisted is up to you (you may opt to persist a payment, or return a response without creating a database record).

In each of the 3 cases, the API response should contain a body with a "declined" payment status that is similar to the successful response above (you can omit the `id` attribute if a "payment" record isn't persisted in the database):

```
{
  "data": {
    "id": "626becac-e2ff-4a7b-95e9-4c8d2e8025da",
    "amount": 0,
    "card_number": "123451234512345",
    "status": "declined"
  }
}
```

# /refunds

The refunds endpoint handles refunding all or part of the money a customer spent on a purchase: this money will later be transferred back to the customer, but that won't concern us here.

Since this is dealing with money, the API only exposes endpoints to create new refunds and view existing refunds: it's not possible to (e.g.) edit or delete refunds from the API.

## Creation

`POST`ing to `/api/payments/PAYMENT_ID/refunds` with valid data should create a new refund. Valid data is defined as follows:

- `amount`: a positive `integer` value representing the monetary amount that is to be refunded to the client's account. It represents the amount in the EUR currency, in cents: a EUR 2.90 refund would be encoded as an amount of `290`.

The above attributes must be wrapped within a `refund` attribute:

```
{
  "refund": {
    "amount": 290
  }
}
```

If the provided attributes are valid and the refund has been created, a `201` HTTP status response will be returned along with the created refund in the body's "data":

```
{
  "data": {
    "id": "a3828107-d407-45b0-86dc-eea7571df3a7",
    "amount": 290
  }
}
```

A "show" endpoint is also exposed by the API, and will return the above response if a GET request is made to `/api/payments/PAYMENT_ID/refunds/a3828107-d407-45b0-86dc-eea7571df3a7`. It's presence is mainly for convenience, as it's implementation won't be part of this challenge.

### Unhappy paths

The API will return a 404 response if the payment against which the refund is being attempted:

- doesn't exist
- has a status other than "approved"

There is no limit on the number of refunds made against a payment: as long as the sum of the refund amounts never exceeds the payment amount, all is well. In other words, all of these scenarios are valid:

- full refund

  1. payment for 10_00
  1. refund for 10_00

- partial refund

  1. payment for 10_00
  1. refund for 2_00
  1. refund for 5_00

- partial refunds up to payment amount
  1. payment for 10_00
  1. refund for 2_00
  1. refund for 5_00
  1. refund for 3_00

The following scenarios, however, would fail with the last refund request listed:

- full refund with excessive amount

  1. payment for 10_00
  1. refund for 11_00

- partial refund with excessive amount
  1. payment for 10_00
  1. refund for 2_00
  1. refund for 9_00

The failed refund attempt would return a 422 status.
