# stacker


Stacker - is a web application that helps users to create custom IT solutions based on open 
source apps and user's custom applications. Users can build their own stack of applications, and 
deploy the final result to their favorite clouds using TryDirect API.

Stacker includes:
1. Security module. User Authorization
2. Application Management
3. Cloud Provider Key Management 
4. docker-compose generator
5. TryDirect API Client
6. Rating module


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