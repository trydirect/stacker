# Stacker


Stacker - is an application that helps users to create custom IT solutions based on dockerized open 
source apps and user's custom applications docker containers. Users can build their own stack of applications, and 
deploy the final result to their favorite clouds using TryDirect API.

Application will consist of:
1. Web UI (Application Stack builder)
2. Back-end RESTful API, includes:
   a. Security module. User Authorization
   b. Application Management
   c. Cloud Provider Key Management 
   d. docker-compose generator
   e. TryDirect API Client
   f. Rating module


Run db migration

```
sqlx migrate run

```

Down migration

```
sqlx migrate revert 
```


Add rating 

```

 curl -vX POST 'http://localhost:8000/rating' -d '{"obj_id": 111, "category": "application", "comment":"some comment", "rate": 10}' --header 'Content-Type: application/json'

```
