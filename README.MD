## User Regsiteration Tool

GitHub OAuth is used to authenticate users. 

Embed the following url in registration page:
```
https://github.com/login/oauth/authorize?client_id=816b35c307fe6ebe19c7&scope=read:user&redirect_uri=https://code.flows.network/webhook/jKRuADFii4naC7ANMFtL/register&state={{codeVerifier}}
```

GitHub sends one-time code to the redirect_uri
```
https://code.flows.network/webhook/jKRuADFii4naC7ANMFtL/register?code=1b2e3f4g5h6i7j8k9l0m
```

The webhook flows function exchanges the code for an access token and retrieves user information.

With use's token, the function can create a new user in the database.



