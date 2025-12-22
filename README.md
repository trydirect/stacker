<a href="https://discord.gg/mNhsa8VdYX"><img alt="Discord" src="https://img.shields.io/discord/578119430391988232?label=discord"></a>
<br>
<div align="center">
<img width="300" src="https://repository-images.githubusercontent.com/448846514/3468f301-0ba6-4b61-9bf1-164c06c06b08"> 
 </div>

# Stacker Project Overview
Stacker - is an application that helps users to create custom IT solutions based on dockerized open 
source apps and user's custom applications docker containers. Users can build their own project of applications, and 
deploy the final result to their favorite clouds using TryDirect API.

## Core Purpose
- Allows users to build projects using both open source and custom Docker containers
- Provides deployment capabilities to various cloud platforms through TryDirect API
- Helps manage and orchestrate Docker-based application stacks

## Main Components

1. **Project Structure**
- Web UI (Stack Builder)
- Command Line Interface
- RESTful API Backend

2. **Key Features**
- User Authentication (via TryDirect OAuth)
- API Client Management
- Cloud Provider Key Management
- Docker Compose Generation
- Project Rating System
- Project Deployment Management

3. **Technical Architecture**
- Written in Rust
- Uses PostgreSQL database
- Implements REST API endpoints
- Includes Docker image validation
- Supports project deployment workflows
- Has RabbitMQ integration for deployment status updates

4. **Data Models**
The core Project model includes:
- Unique identifiers (id, stack_id)
- User identification
- Project metadata (name, body, request_json)
- Timestamps (created_at, updated_at)

5. **API Endpoints**
- `/project` - Project management
- `/rating` - Rating system
- `/client` - API client management
- `/project/deploy` - Deployment handling
- `/project/deploy/status` - Deployment status tracking

The project appears to be a sophisticated orchestration platform that bridges the gap between Docker container management and cloud deployment, with a focus on user-friendly application stack building and management.

This is a high-level overview based on the code snippets provided. The project seems to be actively developed with features being added progressively, as indicated by the TODO sections in the documentation.


## How to start 


## Structure

Stacker (User's dashboard) - Management 
----------
Authentication through TryDirect OAuth
/api/auth checks client's creds, api token, api secret
/apiclient (Create/Manage user's api clients) example: BeerMaster (email, callback)
/rating   


Stacker (API) - Serves API clients 
----------
Authentication made through TryDirect OAuth, here we have only client 
Database (Read only)
Logging/Tracing (Files) / Quickwit for future 
/project (WebUI, as a result we have a JSON)
/project/deploy -> sends deploy command to TryDirect Install service 
/project/deploy/status - get installation progress (rabbitmq client),

#### TODO 
Find out how to get user's token for queue
Limit Requests Frequency (Related to user's settings/role/group etc)
Callback event, example: subscribe on get_report (internally from rabbitmq client),


main client (AllMasters) ->  client (Beermaster) 


#### Run db migration

```
sqlx migrate run

```


#### Down migration

```
sqlx migrate revert 
```


## CURL examples


#### Authentication 


curl -X POST 


#### Rate Product 

```

 curl -vX POST 'http://localhost:8000/rating' -d '{"obj_id": 1, "category": "application", "comment":"some comment", "rate": 10}' --header 'Content-Type: application/json'

```


#### Deploy 
```
curl -X POST -H "Content-Type: application/json" -d @tests/mock_data/custom-stack-payload.json http://127.0.0.1:8000/project -H "Authorization: Bearer $TD_BEARER"
```


#### Create API Client
```
curl -X POST http://localhost:8000/client  --header 'Content-Type: application/json' -H "Authorization: Bearer $TD_BEARER"
```


test client deploy
http://localhost:8000/test/deploy


Test casbin rule
```
cargo r --bin console --features=explain debug casbin --path /client --action POST --subject admin_petru
```
