use hyper;
use typemap;
use super::Error;
pub use hyper::method::Method;

#[derive(Clone)]
pub struct Request {
    pub method: Method,
    path: String,
    query: Option<String>,
    data: typemap::ShareCloneMap,
}

impl Request {
    pub fn new(method: Method, path: String) -> Request {
        let mut path_parts = path.split("?");
        Request {
            method: method,
            path: path_parts.nth(0).unwrap().into(),
            query: path_parts.nth(0).map(|q| q.into()),
            data: typemap::ShareCloneMap::custom(),
        }
    }

    pub fn build(r: hyper::server::Request) -> Result<Request, Error> {
        use hyper::uri::RequestUri::*;

        if let AbsolutePath(path) = r.uri {
            Ok(Request::new(r.method, path))
        } else {
            Err(Error::UnsupportedRequestPathFormat)
        }
    }

    pub fn path(&self) -> &str {
        self.path.as_str()
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_ref().map(|q| q.as_str())
    }

    pub fn get<T>(&self) -> Option<&T::Value>
        where T: typemap::Key,
              T::Value: Clone + Send + Sync
    {
        self.data.get::<T>()
    }

    pub fn set<T>(&mut self, value: T::Value)
        where T: typemap::Key,
              T::Value: Clone + Send + Sync
    {
        self.data.insert::<T>(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_bare_path() {
        let r = Request::new(Method::Get, "/this/is/the/path".into());

        assert_eq!("/this/is/the/path", r.path());
        assert_eq!(None, r.query())
    }

    #[test]
    fn test_request_with_query() {
        let r = Request::new(Method::Get,
                             "/this/is/the/path?and_this_is_the_query".into());

        assert_eq!("/this/is/the/path", r.path());
        assert_eq!(Some("and_this_is_the_query"), r.query());
    }
}
