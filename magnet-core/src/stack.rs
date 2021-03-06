use hyper;
use typemap;
use super::{Request, Response, MagnetResult};

pub trait Responder: Send + Sync {
    fn call(&self, _stack: &Stack, _request: &Request) -> MagnetResult<Option<Response>> {
        Ok(None)
    }
}

pub trait Before: Send + Sync {
    fn call(&self, _stack: &Stack, _request: &mut Request) -> MagnetResult<()> {
        Ok(())
    }
}

pub trait After: Send + Sync {
    fn call(&self, _stack: &Stack, _response: &mut Response) -> MagnetResult<()> {
        Ok(())
    }
}

pub struct Stack {
    befores: Vec<Box<Before>>,
    substacks: Vec<Stack>,
    responders: Vec<Box<Responder>>,
    afters: Vec<Box<After>>,
    env: typemap::ShareMap,
}

impl Stack {
    pub fn new() -> Stack {
        Stack {
            befores: vec![],
            substacks: vec![],
            responders: vec![],
            afters: vec![],
            env: typemap::ShareMap::custom(),
        }
    }

    pub fn get<T>(&self) -> Option<&T::Value>
        where T: typemap::Key,
              T::Value: Clone + Send + Sync
    {
        self.env.get::<T>()
    }

    pub fn set<T>(&mut self, value: T::Value)
        where T: typemap::Key,
              T::Value: Clone + Send + Sync
    {
        self.env.insert::<T>(value);
    }

    pub fn before<B: Before + 'static>(&mut self, before: B) -> &mut Stack {
        self.befores.push(Box::new(before) as Box<Before>);
        self
    }

    pub fn after<A: After + 'static>(&mut self, after: A) -> &mut Stack {
        self.afters.push(Box::new(after) as Box<After>);
        self
    }

    pub fn mount(&mut self, stack: Stack) -> &mut Stack {
        self.substacks.push(stack);
        self
    }

    pub fn add<R: Responder + 'static>(&mut self, responder: R) -> &mut Stack {
        self.responders.push(Box::new(responder) as Box<Responder>);
        self
    }

    pub fn run(&self, request: &mut Request) -> MagnetResult<Option<Response>> {
        for before in &self.befores {
            try!(before.call(&self, request));
        }
        for sub in &self.substacks {
            let mut sub_request = request.clone();
            match sub.run(&mut sub_request) {
                Ok(None) => {}
                Ok(Some(response)) => return self.invoke_afters(response).map(|r| Some(r)),
                Err(err) => return Err(err),
            }
        }
        for responder in &self.responders {
            match responder.call(&self, &request) {
                Ok(None) => {} // not handled, continue
                Ok(Some(response)) => return self.invoke_afters(response).map(|r| Some(r)),
                Err(err) => return Err(err),
            }
        }
        Ok(None)
    }

    fn invoke_afters(&self, mut response: Response) -> MagnetResult<Response> {
        for after in &self.afters {
            try!(after.call(&self, &mut response));
        }
        Ok(response)
    }
}

impl hyper::server::Handler for Stack {
    fn handle(&self,
              hyper_request: hyper::server::Request,
              mut hyper_response: hyper::server::Response) {

        if let Ok(mut request) = Request::build(hyper_request) {
            match self.run(&mut request) {
                Ok(None) => {
                    *hyper_response.status_mut() = hyper::status::StatusCode::NotFound;
                    hyper_response.send(b"Not found").unwrap();
                }
                Ok(Some(response)) => {
                    *hyper_response.headers_mut() = response.headers;
                    *hyper_response.status_mut() = response.status;
                    hyper_response.send(response.body.as_bytes()).unwrap();
                }
                Err(_) => {
                    *hyper_response.status_mut() = hyper::status::StatusCode::InternalServerError;
                }
            }
        } else {
            *hyper_response.status_mut() = hyper::status::StatusCode::InternalServerError;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::request::{Request, Method};
    use ::response::{Response, Status};
    use ::MagnetResult;
    use ::Error;

    struct TestBefore {
        return_error: bool,
    }

    impl Before for TestBefore {
        fn call(&self, _stack: &Stack, _request: &mut Request) -> MagnetResult<()> {
            if self.return_error {
                Err(Error::Generic("Something happened".into()))
            } else {
                Ok(())
            }
        }
    }

    struct TestAfter {
        return_error: bool,
    }

    impl After for TestAfter {
        fn call(&self, _stack: &Stack, _response: &mut Response) -> MagnetResult<()> {
            if self.return_error {
                Err(Error::Generic("Something happened".into()))
            } else {
                Ok(())
            }
        }
    }

    struct OkSomeResponder;

    impl Responder for OkSomeResponder {
        fn call(&self, _stack: &Stack, _request: &Request) -> MagnetResult<Option<Response>> {
            Ok(Some(Response::build(Status::Ok).end()))
        }
    }

    struct OkNoneResponder;

    impl Responder for OkNoneResponder {
        fn call(&self, _stack: &Stack, _request: &Request) -> MagnetResult<Option<Response>> {
            Ok(None)
        }
    }

    struct ErrorResponder;

    impl Responder for ErrorResponder {
        fn call(&self, _stack: &Stack, _request: &Request) -> MagnetResult<Option<Response>> {
            Err(Error::Generic("Something went wrong".into()))
        }
    }

    #[test]
    fn test_error_in_before() {
        let mut stack = Stack::new();
        let mut req = Request::new(Method::Get, "/".into());
        stack.before(TestBefore { return_error: true });
        assert!(stack.run(&mut req).is_err());
    }

    #[test]
    fn test_result_in_responder() {
        let mut stack = Stack::new();
        let mut req = Request::new(Method::Get, "/".into());
        stack.add(OkSomeResponder);
        assert!(stack.run(&mut req).is_ok());
    }

    #[test]
    fn test_none_in_responder() {
        let mut stack = Stack::new();
        let mut req = Request::new(Method::Get, "/".into());
        stack.add(OkNoneResponder);
        assert!(stack.run(&mut req).is_ok());
    }

    #[test]
    fn test_error_in_responder() {
        let mut stack = Stack::new();
        let mut req = Request::new(Method::Get, "/".into());
        stack.add(ErrorResponder);
        assert!(stack.run(&mut req).is_err());
    }

    #[test]
    fn test_error_in_after() {
        let mut stack = Stack::new();
        let mut req = Request::new(Method::Get, "/".into());
        stack.add(OkSomeResponder);
        stack.after(TestAfter { return_error: true });
        assert!(stack.run(&mut req).is_err());
    }
}
